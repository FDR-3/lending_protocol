import * as anchor from "@coral-xyz/anchor"
import { Program } from "@coral-xyz/anchor"
import { LendingProtocol } from "../target/types/lending_protocol"
import { PythMock } from "../target/types/pyth_mock"
import { assert } from "chai"
import * as fs from 'fs'
import { PublicKey, LAMPORTS_PER_SOL, Transaction, Keypair, SystemProgram } from '@solana/web3.js'
import { Token, ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID } from "@solana/spl-token"

//IMPORTANT: #pyth-mock (in Anchor.toml) should be uncommented out when not testing on local net or this test file won't run properly

describe("lending_protocol", () =>
{
  //Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env())

  const program = anchor.workspace.LendingProtocol as Program<LendingProtocol>
  const mockProgram = anchor.workspace.PythMock as Program<PythMock>
  const mockedPythAccountSpace = 134
  const notCEOErrorMsg = "Only the CEO can call this function"
  const feeOnInterestEarnedRateTooHighMsg = "The fee on interest earned rate can't be greater than 100%"
  const feeOnInterestEarnedRateTooLowMsg = "ERR_OUT_OF_RANGE"
  const expectedThisAccountToExistErrorMsg = "The program expected this account to be already initialized"
  const insufficientFundsErrorMsg = "You can't withdraw more funds than you've deposited or an amount that would expose you to liquidation on purpose"
  const incorrentTabAndPythPriceUpdateAccountsErrorMsg = "You must provide all of the sub user's tab accounts and Pyth price update accounts"
  const ataDoesNotExistErrorMsg = "failed to get token account balance: Invalid param: could not find account"
  const incorrectOrderOfTabAccountsErrorMsg = "You must provide the sub user's tab accounts ordered by user_tab_account_index"
  const accountNameTooLongErrorMsg = "Lending User Account name can't be longer than 25 characters"

  const solTokenMintAddress = new PublicKey("So11111111111111111111111111111111111111112")
  const solTokenDecimalAmount = 9
  const twoSol = new anchor.BN(LAMPORTS_PER_SOL * 2)
  const solTestPrice = new anchor.BN(12345)
  const solTestConf = new anchor.BN(12345)
  var solPythPriceUpdateAccountKeypair: Keypair
  var solLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var solPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  
  var usdcMint = undefined
  const usdcTokenDecimalAmount = 6
  const tenUSDC = new anchor.BN(10_000_000)
  const tenKUSDC = 10_000_000_000
  const usdcTestPrice = new anchor.BN(12345)
  const usdcTestConf = new anchor.BN(12345)
  var usdcPythPriceUpdateAccountKeypair: Keypair
  var usdcLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var usdcPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

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

  //Load the keypair from config file
  const keypairPath = '/home/fdr-3/.config/solana/id.json';
  const keypairData = JSON.parse(fs.readFileSync(keypairPath, 'utf8'));
  const testingWalletKeypair = Keypair.fromSecretKey(Uint8Array.from(keypairData))
  const successorWalletKeypair = anchor.web3.Keypair.generate()

  before(async () =>
  {
    //Fund Successor Wallet
    console.log("Funding Sucessor Wallet")
    await airDropSol(successorWalletKeypair.publicKey)

    //Create a new USDC Mint for testing
    console.log("Creating a USDC Token Mint and ATA for Testing")

    usdcMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      program.provider.publicKey, // Mint authority (who can mint tokens)
      null, //Freeze authority (optional)
      usdcTokenDecimalAmount, //Decimals for USDC
      TOKEN_PROGRAM_ID //SPL Token program ID
    )

    const walletATA = await deriveWalletATA(successorWalletKeypair.publicKey, usdcMint.publicKey)
    await createATAForWallet(successorWalletKeypair, usdcMint.publicKey, walletATA)
    await mintUSDCToWallet(usdcMint.publicKey, walletATA)

    //Get Solana Block Chain latest time stamp
    const slot = await program.provider.connection.getSlot();
    const timestamp = new anchor.BN(await program.provider.connection.getBlockTime(slot));

    //Mock Sol Pyth Price Update Account
    console.log("Setting up SOL Mocked Pyth Price Update Account")

    solPythPriceUpdateAccountKeypair = await createMockedPythPriceUpdateAccount()

    await updateMockedPriceUpdateV2Account
    (
      solPythPriceUpdateAccountKeypair,
      solTestPrice,
      solTestConf,
      solTokenDecimalAmount
    )
    
    solPythPriceUpdateRemainingAccount = 
    {
      pubkey: solPythPriceUpdateAccountKeypair.publicKey,
      isSigner: false,
      isWritable: true
    }

    //Mock USDC Pyth Price Update Account
    console.log("Setting up USDC Mocked Pyth Price Update Account")

    usdcPythPriceUpdateAccountKeypair = await createMockedPythPriceUpdateAccount()

    await updateMockedPriceUpdateV2Account
    (
      usdcPythPriceUpdateAccountKeypair,
      usdcTestPrice,
      usdcTestConf,
      usdcTokenDecimalAmount
    )

    usdcPythPriceUpdateRemainingAccount = 
    {
      pubkey: usdcPythPriceUpdateAccountKeypair.publicKey,
      isSigner: false,
      isWritable: true
    }

    console.log("Setup Complete")
  })

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
      .accounts({signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
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
    await program.methods.passOnLendingProtocolCeo(successorWalletKeypair.publicKey).rpc()
    
    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOAccountPDA())
    assert(ceoAccount.address.toBase58() == successorWalletKeypair.publicKey.toBase58())
  })
  
  it("Passes back the Lending Protocol CEO Account", async () => 
  {
    await program.methods.passOnLendingProtocolCeo(program.provider.publicKey)
    .accounts({signer: successorWalletKeypair.publicKey})
    .signers([successorWalletKeypair])
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
      .accounts({signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
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
      await program.methods.addTokenReserve(solTokenMintAddress, solTokenDecimalAmount, solPythPriceUpdateAccountKeypair.publicKey, borrowAPY5Percent, globalLimit1)//IDE complains about ByteArray but still works
      .accounts({mint: solTokenMintAddress, signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
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
    await program.methods.addTokenReserve(solTokenMintAddress, solTokenDecimalAmount, solPythPriceUpdateAccountKeypair.publicKey, borrowAPY5Percent, globalLimit1)//IDE complains about ByteArray but still works
    .accounts({mint: solTokenMintAddress})
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.pythFeedAddress.toBase58() == solPythPriceUpdateAccountKeypair.publicKey.toBase58())
    assert(tokenReserve.borrowApy == borrowAPY5Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit1))
  })

  it("Verifies That Only the CEO Can Update the Token Reserve", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.updateTokenReserve(solTokenMintAddress, borrowAPY7Percent, globalLimit1)
      .accounts({signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
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
    await program.methods.updateTokenReserve(solTokenMintAddress, borrowAPY7Percent, globalLimit2).rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.borrowApy == borrowAPY7Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit2))
  })

  it("Verifies That a SubMarket Can't be Created With a Fee on Interest Rate Higher than 100%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createSubMarket(solTokenMintAddress, testSubMarketIndex, program.provider.publicKey, feeRateAbove100Percent).rpc()
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
      await program.methods.createSubMarket(solTokenMintAddress, testSubMarketIndex, program.provider.publicKey, feeRateBelove0Percent).rpc()
    }
    catch(error)
    {
      errorMessage = error.code
    }

    assert(errorMessage == feeOnInterestEarnedRateTooLowMsg)
  })

  it("Creates a wSOL SubMarket", async () => 
  {
    await program.methods.createSubMarket(solTokenMintAddress, testSubMarketIndex, program.provider.publicKey, feeRate4Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == feeRate4Percent)
    assert(subMarket.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  it("Edits a wSOL SubMarket", async () => 
  {
    await program.methods.editSubMarket(solTokenMintAddress, testSubMarketIndex, successorWalletKeypair.publicKey, feeRate100Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == feeRate100Percent)
    assert(subMarket.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  //Because the SubMarket account is derived from the signer calling the function (and not passed into the function on trust), it's never possible to even try to edit someone else's submarket
  it("Verifies That a SubMarket Can Only be Edited by the Owner", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.editSubMarket(solTokenMintAddress, testSubMarketIndex, successorWalletKeypair.publicKey, feeRate100Percent)
      .accounts({signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
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
    await program.methods.depositTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, accountName)
    .accounts({mint: solTokenMintAddress, signer: successorWalletKeypair.publicKey})
    .signers([successorWalletKeypair])
    .rpc()
   
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(twoSol))
  
    const tokenReserveATA = await deriveWalletATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == twoSol.toNumber())

    const lendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserAccount.accountName == accountName)
    assert(lendingUserAccount.tabAccountCount == 1)

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 0)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(twoSol))

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTokenMintAddress,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(twoSol))
    assert(lendingUserMonthlyStatementAccount.monthlyDepositedAmount.eq(twoSol))

    //Populate sol remaining account
    const solLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )

    solLendingUserTabRemainingAccount = 
    {
      pubkey: solLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }
  })

  it("Verifies a User Can't Have an Account Name Longer Than 25 Characters", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.editLendingUserAccountName(testUserAccountIndex, accountName26Characters)
      .accounts({signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
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
    .accounts({signer: successorWalletKeypair.publicKey})
    .signers([successorWalletKeypair])
    .rpc()

    const lendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      successorWalletKeypair.publicKey,
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
      await program.methods.withdrawTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tooMuchSol)
      .accounts({mint: solTokenMintAddress, signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }
 
    assert(errorMessage == insufficientFundsErrorMsg)
  })

  it("Verifies a User Can't Withdraw wSOL Funds Without Showing All of Their Tabs and Price Update Accounts", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.withdrawTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol)
      .accounts({mint: solTokenMintAddress, signer: successorWalletKeypair.publicKey})
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == incorrentTabAndPythPriceUpdateAccountsErrorMsg)
  })

  it("Withdraws wSOL From the Token Reserve", async () => 
  {
    const remainingAccounts = [solLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount]

    await program.methods.withdrawTokens(
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      twoSol
    )
    .accounts({ mint: solTokenMintAddress, signer: successorWalletKeypair.publicKey })
    .remainingAccounts(remainingAccounts)
    .signers([successorWalletKeypair])
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))

    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 0)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(bnZero))

    const tokenReserveATA = await deriveWalletATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == 0)

    var errorMessage = ""

    const userATA = await deriveWalletATA(successorWalletKeypair.publicKey, solTokenMintAddress, true)
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
      solTokenMintAddress,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(bnZero))
    assert(lendingUserMonthlyStatementAccount.monthlyWithdrawalAmount.eq(twoSol))

    //Verify that wrapped SOL ATA for User was closed since it was empty
    assert(errorMessage == ataDoesNotExistErrorMsg)

    var userBalance = await program.provider.connection.getBalance(successorWalletKeypair.publicKey)

    assert(userBalance >= 9999)
  })

  it("Adds a USDC Token Reserve", async () => 
  {
    await program.methods.addTokenReserve(usdcMint.publicKey, usdcTokenDecimalAmount, usdcPythPriceUpdateAccountKeypair.publicKey, borrowAPY5Percent, globalLimit1)//IDE complains about ByteArray but still works
    .accounts({mint: usdcMint.publicKey})
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
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
    .accounts({mint: usdcMint.publicKey, signer: successorWalletKeypair.publicKey})
    .signers([successorWalletKeypair])
    .rpc()
   
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(tenUSDC))

    const lendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserAccount.tabAccountCount == 2)

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
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
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(tenUSDC))
    assert(lendingUserMonthlyStatementAccount.monthlyDepositedAmount.eq(tenUSDC))

    const tokenReserveATA = await deriveWalletATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == tenUSDC.toNumber())

    //Populate USDC remaining account
    const usdcLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )

    usdcLendingUserTabRemainingAccount = 
    {
      pubkey: usdcLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }
  })

  it("Verifies you Must Pass in the User Tab Accounts in the Order They Were Created", async () => 
  {
    var errorMessage = ""

    try
    {
      const remainingAccounts = [usdcLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount, solLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount]
      
      await program.methods.withdrawTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tenUSDC)
      .accounts({mint: usdcMint.publicKey, signer: successorWalletKeypair.publicKey})
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
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
    const remainingAccounts = [solLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, usdcLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]

    await program.methods.withdrawTokens(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      tenUSDC
    )
    .accounts({mint: usdcMint.publicKey, signer: successorWalletKeypair.publicKey})
    .remainingAccounts(remainingAccounts)
    .signers([successorWalletKeypair])
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))

    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
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
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(bnZero))
    assert(lendingUserMonthlyStatementAccount.monthlyWithdrawalAmount.eq(tenUSDC))

    const userATA = await deriveWalletATA(successorWalletKeypair.publicKey, usdcMint.publicKey, true)
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

    //2. Send the transaction
    await program.provider.sendAndConfirm(transaction);
  }

  async function createMockedPythPriceUpdateAccount()
  {
    const newAccount = Keypair.generate();
    const createTx = new Transaction().add(
      SystemProgram.createAccount({
        fromPubkey: program.provider.publicKey,
        newAccountPubkey: newAccount.publicKey,
        programId: mockProgram.programId,
        lamports:
          await program.provider.connection.getMinimumBalanceForRentExemption(
            mockedPythAccountSpace
          ),
        space: mockedPythAccountSpace,
      })
    );

    await program.provider.sendAndConfirm(createTx, [testingWalletKeypair, newAccount]);
  
    return newAccount;
  }

  async function updateMockedPriceUpdateV2Account(
  mockedPythKeyPair: Keypair,
  price: anchor.BN,
  conf: anchor.BN,
  exponent: number)
  {
    //Get latest block chain timestamp.
    const slot = await program.provider.connection.getSlot();
    const timeStamp = await program.provider.connection.getBlockTime(slot);
    //const timeStamp = Math.floor(Date.now() / 1000); 

    const publish_time = new anchor.BN(timeStamp);
    const prev_publish_time = new anchor.BN(timeStamp - 1);

    // Allocate a 136-byte buffer.
    const buf = Buffer.alloc(mockedPythAccountSpace);
    let offset = 0;
    
    //1. Write the 8-byte Pyth Discriminator/Magic Number. (8 bytes)
    const discriminator = Buffer.from([34, 241, 35, 99, 157, 126, 244, 205]);
    discriminator.copy(buf, offset);
    offset += 8; // offset = 8
    
    //2. Write the write_authority (32 bytes).
    const writeAuthority = PublicKey.unique().toBuffer();
    writeAuthority.copy(buf, offset);
    offset += 32; // offset = 40
    
    //3. Write verification_level (1 byte tag).
    buf.writeUInt8(1, offset); // tag '1' for Full verification (1 byte)
    offset += 1; // offset = 41
    
    //PriceFeedMessage starts here (Total 92 bytes):
    //4. feedID (32 bytes)
    const feedID = mockedPythKeyPair.publicKey; 
    feedID.toBuffer().copy(buf, offset);
    offset += 32; // offset = 76
    
    //6. price (i64, 8 bytes)
    price.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8; // offset = 84
    
    //7. conf (u64, 8 bytes)
    conf.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8; // offset = 92
    
    //8. exponent (i32, 4 bytes)
    buf.writeInt32LE(exponent, offset);
    offset += 4; // offset = 96
    
    //9. publish_time (i64, 8 bytes)
    publish_time.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8; // offset = 104
    
    //10. prev_publish_time (i64, 8 bytes)
    prev_publish_time.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8; // offset = 112
    
    //11. ema_price (i64, 8 bytes)
    price.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8; // offset = 120
    
    //12. ema_conf (u64, 8 bytes)
    conf.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8; // offset = 128
    
    //13. posted_slot (u64, 8 bytes)
    (new anchor.BN(0)).toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8; // offset = 136

    //Write the buffer data to the mock account
    await mockProgram.methods.setMockedPythPriceUpdateAccount(buf)
    .accounts({mockedPythPriceUpdatePda: mockedPythKeyPair.publicKey})
    .signers([mockedPythKeyPair])
    .rpc()
  }

  const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms))
  var counter = 0
  
  async function sleepFunction()
  {
    console.log('Start sleep: ', counter)
    await sleep(5000) // Sleep for 5 seconds
    console.log('End sleep: ', counter)
    counter += 1
  }
})