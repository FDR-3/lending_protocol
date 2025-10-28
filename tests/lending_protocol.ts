import * as anchor from "@coral-xyz/anchor"
import { Program } from "@coral-xyz/anchor"
import { LendingProtocol } from "../target/types/lending_protocol"
import { assert } from "chai"
import * as fs from 'fs'
import { PublicKey, LAMPORTS_PER_SOL, Transaction, Keypair } from '@solana/web3.js'
import { Token, ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID } from "@solana/spl-token"

describe("lending_protocol", () =>
{
  //Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env())

  const program = anchor.workspace.LendingProtocol as Program<LendingProtocol>
  const notCEOErrorMsg = "Only the CEO can call this function"
  const feeOnInterestEarnedRateTooHighMsg = "The fee on interest earned rate can't be greater than 100%"
  const feeOnInterestEarnedRateTooLowMsg = "ERR_OUT_OF_RANGE"
  const expectedThisAccountToExistErrorMsg = "The program expected this account to be already initialized"
  const insufficientFundsErrorMsg = "You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose"
  const incorrentTabAccountsErrorMsg = "You must provide all of the sub user's tab accounts"
  const ataDoesNotExistErrorMsg = "failed to get token account balance: Invalid param: could not find account"
  const incorrectOrderOfTabAccountsErrorMsg = "You must provide the sub user's tab accounts ordered by user_tab_account_index"
  const accountNameTooLongErrorMsg = "Lending User Account name can't be longer than 25 characters"

  const SOLTokenMintAddress = new PublicKey("So11111111111111111111111111111111111111112")
  const SOLTokenDecimalAmount = 9
  const twoSol = new anchor.BN(LAMPORTS_PER_SOL * 2)
  
  var usdcMint = undefined
  const usdcDecimalAmount = 6
  const tenUSDC = new anchor.BN(10_000_000)
  const tenKUSDC = 10_000_000_000

  const borrowAPY5Percent = 500
  const borrowAPY7Percent = 700
  const globalLimit1 = new anchor.BN(10_000_000_000)
  const globalLimit2 = new anchor.BN(20_000_000_000)

  const feeRateAbove100Percent = 10001
  const feeRateBelove0Percent = -1
  const feeRate4Percent = 400
  const feeRate100Percent = 10000

  const testSubMarketIndex = 4
  const testUserAccountIndex = 7
  const bnZero = new anchor.BN(0)
  const statementMonth = 1
  const statementYear = 2025
  const newStatementMonth = 2
  const newStatementYear = 2044
  const accountName = "Account 1"
  const accountName25Characters = "Lorem ipsum dolor sit ame"
  const accountName26Characters = "Lorem ipsum dolor sit amet"

  let successorWallet = anchor.web3.Keypair.generate()

  it("Initializes Lending Protocol", async () => 
  {
    await program.methods.initializeLendingProtocol(statementMonth, statementYear).rpc()

    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOAccountPDA())
    assert(ceoAccount.address.toBase58() == program.provider.publicKey.toBase58())

    var lendingProtocol = await program.account.lendingProtocol.fetch(getLendingProtocolPDA())
    assert(lendingProtocol.currentStatementMonth == statementMonth)
    assert(lendingProtocol.currentStatementYear == statementYear)
  })

  it("Verifies That Only the CEO Can Pass On Account", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.passOnLendingProtocolCeo(program.provider.publicKey)
      .accounts({signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notCEOErrorMsg)
  })

  it("Passes on the Lending Protocol CEO Account", async () => 
  {
    await airDropSol(successorWallet.publicKey)

    await program.methods.passOnLendingProtocolCeo(successorWallet.publicKey).rpc()
    
    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOAccountPDA())
    assert(ceoAccount.address.toBase58() == successorWallet.publicKey.toBase58())
  })
  
  it("Passes back the Lending Protocol CEO Account", async () => 
  {
    await program.methods.passOnLendingProtocolCeo(program.provider.publicKey)
    .accounts({signer: successorWallet.publicKey})
    .signers([successorWallet])
    .rpc()
    
    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOAccountPDA())
    assert(ceoAccount.address.toBase58() == program.provider.publicKey.toBase58())
  })

  it("Verifies That Only the CEO Can Update the Lending Protocol Statement Year", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.updateCurrentStatementMonthAndYear(newStatementMonth, newStatementYear)
      .accounts({signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notCEOErrorMsg)
  })

  it("Updates Lending Lending Protocol Statement Year", async () => 
  {
    await program.methods.updateCurrentStatementMonthAndYear(newStatementMonth, newStatementYear).rpc()

    var lendingProtocol = await program.account.lendingProtocol.fetch(getLendingProtocolPDA())

    assert(lendingProtocol.currentStatementMonth == newStatementMonth)
    assert(lendingProtocol.currentStatementYear == newStatementYear)
  })

  it("Verifies That Only the CEO Can Add a Token Reserve", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.addTokenReserve(SOLTokenMintAddress, SOLTokenDecimalAmount, borrowAPY5Percent, globalLimit1)
      .accounts({mint: SOLTokenMintAddress, signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notCEOErrorMsg)
  })
  
  it("Adds a wSOL Token Reserve", async () => 
  {
    await program.methods.addTokenReserve(SOLTokenMintAddress, SOLTokenDecimalAmount, borrowAPY5Percent, globalLimit1)
    .accounts({mint: SOLTokenMintAddress})
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(SOLTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == SOLTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == SOLTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.borrowApy == borrowAPY5Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit1))
  })

  it("Verifies That Only the CEO Can Update the Token Reserve", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.updateTokenReserve(SOLTokenMintAddress, borrowAPY7Percent, globalLimit1)
      .accounts({signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notCEOErrorMsg)
  })

  it("Updates Token Reserve Borrow APY and Global Limit", async () => 
  {
    await program.methods.updateTokenReserve(SOLTokenMintAddress, borrowAPY7Percent, globalLimit2).rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(SOLTokenMintAddress))
    assert(tokenReserve.borrowApy == borrowAPY7Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit2))
  })

  it("Verifies That a SubMarket Can't be Created With a Fee on Interest Rate Higher than 100%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createSubMarket(SOLTokenMintAddress, testSubMarketIndex, program.provider.publicKey, feeRateAbove100Percent).rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == feeOnInterestEarnedRateTooHighMsg)
  })

  it("Verifies That a SubMarket Can't be Created With a Fee on Interest Rate Below 0%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createSubMarket(SOLTokenMintAddress, testSubMarketIndex, program.provider.publicKey, feeRateBelove0Percent).rpc()
    }
    catch(error)
    {
      errorMessage = error.code
    }

    assert(errorMessage == feeOnInterestEarnedRateTooLowMsg)
  })

  it("Creates a wSOL SubMarket", async () => 
  {
    await program.methods.createSubMarket(SOLTokenMintAddress, testSubMarketIndex, program.provider.publicKey, feeRate4Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(SOLTokenMintAddress, program.provider.publicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == feeRate4Percent)
    assert(subMarket.tokenMintAddress.toBase58() == SOLTokenMintAddress.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  it("Edits a wSOL SubMarket", async () => 
  {
    await program.methods.editSubMarket(SOLTokenMintAddress, testSubMarketIndex, successorWallet.publicKey, feeRate100Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(SOLTokenMintAddress, program.provider.publicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == successorWallet.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == feeRate100Percent)
    assert(subMarket.tokenMintAddress.toBase58() == SOLTokenMintAddress.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  //Because the SubMarket account is derived from the signer calling the function (and not passed into the function on trust), it's never possible to even try to edit someone else's submarket
  it("Verifies That a SubMarket Can Only be Edited by the Owner", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.editSubMarket(SOLTokenMintAddress, testSubMarketIndex, successorWallet.publicKey, feeRate100Percent)
      .accounts({signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == expectedThisAccountToExistErrorMsg)
  })

  it("Deposits wSOL Into the Token Reserve", async () => 
  {
    await program.methods.depositTokens(SOLTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, accountName)
    .accounts({mint: SOLTokenMintAddress, signer: successorWallet.publicKey})
    .signers([successorWallet])
    .rpc()
   
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(SOLTokenMintAddress))

    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == SOLTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == SOLTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(twoSol))

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      SOLTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWallet.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenMintAddress.toBase58() == SOLTokenMintAddress.toBase58())
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 0)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(twoSol))

    const tokenReserveATA = await deriveWalletATA(getTokenReservePDA(SOLTokenMintAddress), SOLTokenMintAddress, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == twoSol.toNumber())

    const lendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserAccount.accountName == accountName)

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      SOLTokenMintAddress,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.currentBalanceAmount.eq(twoSol))
    assert(lendingUserMonthlyStatementAccount.monthlyDepositedAmount.eq(twoSol))
  })

  it("Verifies a User Can't Have an Account Name Longer Than 25 Characters", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.editLendingUserAccountName(testUserAccountIndex, accountName26Characters)
      .accounts({signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == accountNameTooLongErrorMsg)
  })

  it("Verifies a User Can Change Their Account Names", async () => 
  {
    await program.methods.editLendingUserAccountName(testUserAccountIndex, accountName25Characters)
    .accounts({signer: successorWallet.publicKey})
    .signers([successorWallet])
    .rpc()

    const lendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserAccount.accountName == accountName25Characters)
  })

  it("Verifies a User Can't Withdraw More wSOL Than They Deposited", async () => 
  {
    var errorMessage = ""
    const tooMuchSol = twoSol.add(new anchor.BN(1))

    try
    {
      await program.methods.withdrawTokens(SOLTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tooMuchSol)
      .accounts({mint: SOLTokenMintAddress, signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }
 
    assert(errorMessage == insufficientFundsErrorMsg)
  })

  it("Verifies a User Can't Withdraw wSOL Funds Without Showing All of Their Tabs", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.withdrawTokens(SOLTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol)
      .accounts({mint: SOLTokenMintAddress, signer: successorWallet.publicKey})
      .signers([successorWallet])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == incorrentTabAccountsErrorMsg)
  })

  it("Withdraws wSOL From the Token Reserve", async () => 
  {
    const wSOLLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      SOLTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWallet.publicKey,
      testUserAccountIndex
    )

    var wSOLLendingUserTabRemainingAccount = 
    {
      pubkey: wSOLLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }

    await program.methods.withdrawTokens(SOLTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol)
    .accounts({mint: SOLTokenMintAddress, signer: successorWallet.publicKey})
    .signers([successorWallet])
    .remainingAccounts([wSOLLendingUserTabRemainingAccount])
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(SOLTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == SOLTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == SOLTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))

    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      SOLTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWallet.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenMintAddress.toBase58() == SOLTokenMintAddress.toBase58())
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 0)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(bnZero))

    const tokenReserveATA = await deriveWalletATA(getTokenReservePDA(SOLTokenMintAddress), SOLTokenMintAddress, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == 0)

    var errorMessage = ""

    const userATA = await deriveWalletATA(successorWallet.publicKey, SOLTokenMintAddress, true)
    try
    {
      await program.provider.connection.getTokenAccountBalance(userATA)
    }
    catch(error)
    {
      errorMessage = error.message
    }

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      SOLTokenMintAddress,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.currentBalanceAmount.eq(bnZero))
    assert(lendingUserMonthlyStatementAccount.monthlyWithdrawalAmount.eq(twoSol))

    //Verify that wrapped SOL ATA for User was closed since it was empty
    assert(errorMessage == ataDoesNotExistErrorMsg)

    var userBalance = await program.provider.connection.getBalance(successorWallet.publicKey)

    assert(userBalance >= 9999)
  })

  //Load the keypair from config file
  const keypairPath = '/home/fdr-3/.config/solana/id.json';
  const keypairData = JSON.parse(fs.readFileSync(keypairPath, 'utf8'));
  const testingWalletKeypair = Keypair.fromSecretKey(Uint8Array.from(keypairData))

  it("Creates A USDC Token Mint For Testing", async () => 
  {
    //Create a new USDC Mint for testing
    usdcMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      program.provider.publicKey, // Mint authority (who can mint tokens)
      null, //Freeze authority (optional)
      usdcDecimalAmount, //Decimals for USDC
      TOKEN_PROGRAM_ID //SPL Token program ID
    )

    const walletATA = await deriveWalletATA(successorWallet.publicKey, usdcMint.publicKey)
    await createATAForWallet(successorWallet, usdcMint.publicKey, walletATA)
    await mintUSDCToWallet(usdcMint.publicKey, walletATA)
  })

  it("Adds a USDC Token Reserve", async () => 
  {
    await program.methods.addTokenReserve(usdcMint.publicKey, usdcDecimalAmount, borrowAPY5Percent, globalLimit1)
    .accounts({mint: usdcMint.publicKey})
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.borrowApy == borrowAPY5Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit1))
  })

  it("Creates a USDC SubMarket", async () => 
  {
    await program.methods.createSubMarket(usdcMint.publicKey, testSubMarketIndex, program.provider.publicKey, feeRate4Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == feeRate4Percent)
    assert(subMarket.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  it("Deposits USDC Into the Token Reserve", async () => 
  {
    await program.methods.depositTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tenUSDC, null)
    .accounts({mint: usdcMint.publicKey, signer: successorWallet.publicKey})
    .signers([successorWallet])
    .rpc()
   
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(tenUSDC))

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWallet.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 1)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(tenUSDC))

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcMint.publicKey,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.currentBalanceAmount.eq(tenUSDC))
    assert(lendingUserMonthlyStatementAccount.monthlyDepositedAmount.eq(tenUSDC))

    const tokenReserveATA = await deriveWalletATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == tenUSDC.toNumber())
  })

  it("Verifies you Must Pass in the User Tab Accounts in the Order They Were Created", async () => 
  {
    var errorMessage = ""

    try
    {
      const wSOLLendingUserTabAccountPDA = getLendingUserTabAccountPDA
      (
        SOLTokenMintAddress,
        program.provider.publicKey,
        testSubMarketIndex,
        successorWallet.publicKey,
        testUserAccountIndex
      )
      const usdcLendingUserTabAccountPDA = getLendingUserTabAccountPDA
      (
        usdcMint.publicKey,
        program.provider.publicKey,
        testSubMarketIndex,
        successorWallet.publicKey,
        testUserAccountIndex
      )

      var wSOLLendingUserTabRemainingAccount = 
      {
        pubkey: wSOLLendingUserTabAccountPDA,
        isSigner: false,
        isWritable: true
      }
      var usdcLendingUserTabRemainingAccount = 
      {
        pubkey: usdcLendingUserTabAccountPDA,
        isSigner: false,
        isWritable: true
      }

      await program.methods.withdrawTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tenUSDC)
      .accounts({mint: usdcMint.publicKey, signer: successorWallet.publicKey})
      .signers([successorWallet])
      .remainingAccounts([usdcLendingUserTabRemainingAccount, wSOLLendingUserTabRemainingAccount])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == incorrectOrderOfTabAccountsErrorMsg)
  })

  it("Withdraws USDC From the Token Reserve", async () => 
  {
    const wSOLLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      SOLTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWallet.publicKey,
      testUserAccountIndex
    )
    const usdcLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWallet.publicKey,
      testUserAccountIndex
    )

    var wSOLLendingUserTabRemainingAccount = 
    {
      pubkey: wSOLLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }
    var usdcLendingUserTabRemainingAccount = 
    {
      pubkey: usdcLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }

    await program.methods.withdrawTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tenUSDC)
    .accounts({mint: usdcMint.publicKey, signer: successorWallet.publicKey})
    .signers([successorWallet])
    .remainingAccounts([wSOLLendingUserTabRemainingAccount, usdcLendingUserTabRemainingAccount])
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))

    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWallet.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 1)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(bnZero))

    const tokenReserveATA = await deriveWalletATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == 0)

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcMint.publicKey,
      successorWallet.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.currentBalanceAmount.eq(bnZero))
    assert(lendingUserMonthlyStatementAccount.monthlyWithdrawalAmount.eq(tenUSDC))

    const userATA = await deriveWalletATA(successorWallet.publicKey, usdcMint.publicKey, true)
    const UserATAAccount = await program.provider.connection.getTokenAccountBalance(userATA)
    assert(parseInt(UserATAAccount.value.amount) == tenKUSDC)
  })

  function getLendingProtocolCEOAccountPDA()
  {
    const [lendingProtocolCEOPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("lendingProtocolCEO")
      ],
      program.programId
    )
    return lendingProtocolCEOPDA
  }

  function getLendingProtocolPDA()
  {
    const [lendingProtocolCEOPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("lendingProtocol")
      ],
      program.programId
    )
    return lendingProtocolCEOPDA
  }

  function getTokenReservePDA(tokenMintAddress: PublicKey)
  {
    const [tokenReservePDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("tokenReserve"),
        tokenMintAddress.toBuffer()

      ],
      program.programId
    )
    return tokenReservePDA
  }

  function getSubMarketPDA(tokenMintAddress: PublicKey, subMarketOwner: PublicKey, subMarketIndex: number)
  {
    const [subMarketPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("subMarket"),
        tokenMintAddress.toBuffer(),
        subMarketOwner.toBuffer(),
        new anchor.BN(subMarketIndex).toBuffer('le', 2)
      ],
      program.programId
    )
    return subMarketPDA
  }

  function getLendingUserAccountPDA(lendingUserAddress: PublicKey, lendingUserAccountIndex: number)
  {
    const [lendingUserTabAccountPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("lendingUserAccount"),
        lendingUserAddress.toBuffer(),
        new anchor.BN(lendingUserAccountIndex).toBuffer('le', 1),
      ],
      program.programId
    )
    return lendingUserTabAccountPDA
  }

  function getlendingUserMonthlyStatementAccountPDA(statementMonth: number, statementYear: number, tokenMintAddress: PublicKey, lendingUserAddress: PublicKey, lendingUserAccountIndex: number)
  {
    const [lendingUserMonthlyStatementAccountPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("userMonthlyStatementAccount"),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        new anchor.BN(statementMonth).toBuffer('le', 1),
        new anchor.BN(statementYear).toBuffer('le', 4),
        tokenMintAddress.toBuffer(),
        lendingUserAddress.toBuffer(),
        new anchor.BN(lendingUserAccountIndex).toBuffer('le', 1),
      ],
      program.programId
    )
    return lendingUserMonthlyStatementAccountPDA
  }

  function getLendingUserTabAccountPDA(tokenMintAddress: PublicKey,
    subMarketOwner: PublicKey,
    subMarketIndex: number,
    lendingUserAddress: PublicKey,
    lendingUserAccountIndex: number)
  {
    const [lendingUserTabAccountPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("lendingUserTabAccount"),
        tokenMintAddress.toBuffer(),
        subMarketOwner.toBuffer(),
        new anchor.BN(subMarketIndex).toBuffer('le', 2),
        lendingUserAddress.toBuffer(),
        new anchor.BN(lendingUserAccountIndex).toBuffer('le', 1),
      ],
      program.programId
    )
    return lendingUserTabAccountPDA
  }

  async function airDropSol(walletPublicKey: PublicKey)
  {
    let token_airdrop = await program.provider.connection.requestAirdrop(walletPublicKey, 
    100 * LAMPORTS_PER_SOL) //1 billion lamports equals 1 SOL

    const latestBlockHash = await program.provider.connection.getLatestBlockhash()
    await program.provider.connection.confirmTransaction
    ({
      blockhash: latestBlockHash.blockhash,
      lastValidBlockHeight: latestBlockHash.lastValidBlockHeight,
      signature: token_airdrop
    })
  }

  async function deriveWalletATA(walletPublicKey: PublicKey, tokenMintAddress: PublicKey, pdaAccount: boolean = false)
  {
    return await Token.getAssociatedTokenAddress
    (
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      tokenMintAddress,
      walletPublicKey,
      pdaAccount
    )
  }

  async function createATAForWallet(walletKeyPair: Keypair, tokenMintAddress: PublicKey, walletATA: PublicKey)
  {
    //1. Add createATA instruction to transaction
    const transaction = new Transaction().add
    (
      Token.createAssociatedTokenAccountInstruction
      (
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        tokenMintAddress,
        walletATA,
        walletKeyPair.publicKey,
        walletKeyPair.publicKey
      )
    )

    //2. Fetch the latest blockhash and set it on the transaction.
    const latestBlockhash = await program.provider.connection.getLatestBlockhash()
    transaction.recentBlockhash = latestBlockhash.blockhash
    transaction.feePayer = walletKeyPair.publicKey

    //3. Sign the transaction
    transaction.sign(walletKeyPair);
    //const signedTransaction = await program.provider.wallet.signTransaction(transaction)

    //4. Send the signed transaction to the network.
    //We get the signature back, which can be used to track the transaction.
    const tx = await program.provider.connection.sendRawTransaction(transaction.serialize())

    await program.provider.connection.confirmTransaction(tx, 'processed')
  }

  async function mintUSDCToWallet(tokenMintAddress: PublicKey, walletATA: PublicKey)
  {
    //1. Add createMintTo instruction to transaction
    const transaction = new Transaction().add
    (
      Token.createMintToInstruction
      (
        TOKEN_PROGRAM_ID,
        tokenMintAddress,
        walletATA,
        program.provider.publicKey,
        [testingWalletKeypair],
        tenKUSDC//$10,000.00
      )
    )

    // 3. Send the transaction
    await program.provider.sendAndConfirm(transaction);
  }
})
