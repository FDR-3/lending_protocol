import * as anchor from "@coral-xyz/anchor"
import { Program } from "@coral-xyz/anchor"
import { LendingProtocol } from "../target/types/lending_protocol"
import { PythMock } from "../target/types/pyth_mock"
import { assert } from "chai"
import * as fs from 'fs'
import { PublicKey, LAMPORTS_PER_SOL, Transaction, Keypair, SystemProgram } from '@solana/web3.js'
import { Token, ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID } from "@solana/spl-token"

describe("lending_protocol", () =>
{
  //The official Token-2022 Program ID
  const TOKEN_2022_PROGRAM_ID = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")

  //Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env())

  const program = anchor.workspace.LendingProtocol as Program<LendingProtocol>
  const mockProgram = anchor.workspace.PythMock as Program<PythMock>
  const mockedPythAccountSpace = 134
  const pythPriceDecimals = 8
  const notCEOErrorMsg = "Only the CEO can call this function"
  const feeOnInterestEarnedRateTooHighMsg = "The fee on interest earned rate can't be greater than 100%"
  const feeOnInterestEarnedRateTooLowMsg = "ERR_OUT_OF_RANGE"
  const globalLimitExceededErrorMsg = "You can't deposit more than the global limit"
  const expectedThisAccountToExistErrorMsg = "The program expected this account to be already initialized"
  const insufficientFundsErrorMsg = "You can't withdraw more funds than you've deposited"
  const incorrentTabAndPythPriceUpdateAccountsErrorMsg = "You must provide all of the sub user's tab accounts and Pyth price update accounts"
  const ataDoesNotExistErrorMsg = "failed to get token account balance: Invalid param: could not find account"
  const debtExceeding70PercentOfCollateralErrorMsg = "You can't withdraw or borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral"
  const insufficientLiquidityErrorMsg = "Not enough liquidity in the Token Reserve for this withdraw or borrow"
  const notLiquidatableErrorMsg = "You can't liquidate an account whose borrow liabilities aren't 80% or more of their deposited collateral"
  const overLiquidationErrorMsg = "You can't repay more than 50% of a liquidati's debt position"
  const tooManyFundsErrorMsg = "You can't pay back more funds than you've borrowed"
  const incorrectOrderOfTabAccountsErrorMsg = "You must provide the sub user's tab accounts ordered by user_tab_account_index"
  const accountNameTooLongErrorMsg = "Lending User Account name can't be longer than 25 characters"
  const negativePythPriceErrorMsg = "Negative Price Detected"
  const unstablePythPriceErrorMsg = "Oracle Price Too Unstable"
  const notFeeCollectorErrorMsg = "Only the Fee Collector can claim the fees"
  const notTreasurerErrorMsg = "Only the Treasurer can call this function"
  
  const solTokenMintAddress = new PublicKey("So11111111111111111111111111111111111111112")
  //const solTokenMintAddress = new PublicKey("9pan9bMn5HatX4EJdBwg9VgCa7Uz5HL8N1m5D3NdXejP")
  const solPythFeedIDBuffer = Buffer.from("ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d", "hex")
  const solPythFeedIDArray = Array.from(Buffer.from("ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d", "hex"))
  const solTokenDecimalAmount = 9
  const oneSol = new anchor.BN(LAMPORTS_PER_SOL)
  const twoSol = new anchor.BN(LAMPORTS_PER_SOL * 2)
  const solTestPrice = BigInt(10_000_000_000)//8 Decimal Price
  const solNegativePrice = BigInt(-10_000_000_000)//8 Decimal Price
  const solLiquidationPrice = BigInt(10_000_000_001)//8 Decimal Price
  const solTestConf = new anchor.BN(245)
  const solUncertainConf = new anchor.BN(200_000_001)
  var solPythPriceUpdateAccountKeypair: Keypair
  var supplierSOLLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerSOLLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var solPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  
  var usdcMint = undefined
  const usdcPythFeedIDBuffer = Buffer.from("eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a", "hex")
  const usdcPythFeedIDArray = Array.from(Buffer.from("eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a", "hex"))
  const usdcTokenDecimalAmount = 6
  const halfBorrowerUSDCAmount = new anchor.BN(35_000_000)
  const borrowerUSDCAmount = new anchor.BN(70_000_000)
  const overBorrowUSDCAmount = new anchor.BN(71_000_000)
  const supplierUSDCAmount = new anchor.BN(100_000_000)
  const tenKUSDC = 10_000_000_000
  const usdcTestPrice = BigInt(100_000_000)//8 Decimal Price
  const usdcNegativePrice = BigInt(-100_000_000)//8 Decimal Price
  const usdcTestConf = new anchor.BN(245)
  const usdcUncertainConf = new anchor.BN(2_000_001)
  var usdcPythPriceUpdateAccountKeypair: Keypair
  var supplierUSDCLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerUSDCLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var usdcPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  const borrowAPY5Percent = 500 //5.00%
  const borrowAPY7Percent = 700 //7.00%
  const globalLimitLow = new anchor.BN(1)
  const globalLimit1 = new anchor.BN(10_000_000_000)
  const globalLimit2 = new anchor.BN(20_000_000_000)

  const subMarketFeeRateAbove100Percent = 10001 //100.01%
  const subMarketFeeRateBelove0Percent = -1 //-0.01%
  const subMarketFeeRate4Percent = 400 //4%
  const subMarketFeeRate100Percent = 10000 //100.00%
  const solvencyInsurancefeeRate4Percent = 400 //4.00%
  const solvencyInsurancefeeRate7Percent = 700 //7.00%

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
  const borrowerWalletKeypair = anchor.web3.Keypair.generate()

  //Test Settings
  const borrowWaitTimeInSeconds = 30
  const useUSDCFixedBorrowAPY = false

  before(async () =>
  {
    //Fund Successor Wallet
    console.log("Funding Sucessor Wallet")
    await airDropSol(successorWalletKeypair.publicKey)

    //Fund Borrower Wallet
    console.log("Funding Borrower Wallet")
    await airDropSol(borrowerWalletKeypair.publicKey)

    //Create a new USDC Mint for testing
    console.log("Creating a USDC Token Mint and ATA for Testing")

    usdcMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      program.provider.publicKey, // Mint authority (who can mint tokens)
      null, //Freeze authority (optional)
      usdcTokenDecimalAmount, //Decimals for USDC
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    //Mint USDC to Successor Wallet
    const successorWalletATA = await deriveATA(successorWalletKeypair.publicKey, usdcMint.publicKey)
    await createATAForWallet(successorWalletKeypair, usdcMint.publicKey, successorWalletATA)
    await mintUSDCToWallet(usdcMint.publicKey, successorWalletATA)

    //Mint USDC to Borrower Wallet
    const borrowerWalletATA = await deriveATA(borrowerWalletKeypair.publicKey, usdcMint.publicKey)
    await createATAForWallet(borrowerWalletKeypair, usdcMint.publicKey, borrowerWalletATA)
    await mintUSDCToWallet(usdcMint.publicKey, borrowerWalletATA)

    //Mock Sol Pyth Price Update Account
    console.log("Setting up SOL Mocked Pyth Price Update Account")

    solPythPriceUpdateAccountKeypair = await createMockedPythPriceUpdateAccount()

    await updateMockedPriceUpdateV2Account
    (
      solPythPriceUpdateAccountKeypair,
      solPythFeedIDBuffer,
      solTestPrice,
      solTestConf,
      pythPriceDecimals
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
      usdcPythFeedIDBuffer,
      usdcTestPrice,
      usdcTestConf,
      pythPriceDecimals
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
      .accounts({ signer: successorWalletKeypair.publicKey })
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
    .accounts({ signer: successorWalletKeypair.publicKey })
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
      .accounts({ signer: successorWalletKeypair.publicKey })
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
      await program.methods.addTokenReserve(solTokenMintAddress, solTokenDecimalAmount, solPythFeedIDArray, borrowAPY5Percent, true, globalLimit1, solvencyInsurancefeeRate4Percent)//IDE complains about ByteArray but still works
      .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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
    await program.methods.addTokenReserve(solTokenMintAddress, solTokenDecimalAmount, solPythFeedIDArray, borrowAPY5Percent, true, globalLimitLow, solvencyInsurancefeeRate4Percent)//IDE complains about ByteArray but still works
    .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.pythFeedId.toString() == solPythFeedIDArray.toString())
    assert(tokenReserve.borrowApy == borrowAPY5Percent)
    assert(tokenReserve.globalLimit.eq(globalLimitLow))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsurancefeeRate4Percent)
  })

  it("Verifies That a SubMarket Can't be Created With a Fee on Interest Rate Higher than 100%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createSubMarket(solTokenMintAddress, testSubMarketIndex, program.provider.publicKey, subMarketFeeRateAbove100Percent).rpc()
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
      await program.methods.createSubMarket(solTokenMintAddress, testSubMarketIndex, program.provider.publicKey, subMarketFeeRateBelove0Percent).rpc()
    }
    catch(error)
    {
      errorMessage = error.code
    }

    assert(errorMessage == feeOnInterestEarnedRateTooLowMsg)
  })

  it("Creates a wSOL SubMarket", async () => 
  {
    await program.methods.createSubMarket(solTokenMintAddress, testSubMarketIndex, program.provider.publicKey, subMarketFeeRate4Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate4Percent)
    assert(subMarket.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  it("Edits a wSOL SubMarket", async () => 
  {
    await program.methods.editSubMarket(solTokenMintAddress, testSubMarketIndex, successorWalletKeypair.publicKey, subMarketFeeRate100Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate100Percent)
    assert(subMarket.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  //Because the SubMarket account is derived from the signer calling the function (and not passed into the function on trust), it's never possible to even try to edit someone else's submarket
  it("Verifies That a SubMarket Can Only be Edited by the Owner", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.editSubMarket(solTokenMintAddress, testSubMarketIndex, successorWalletKeypair.publicKey, subMarketFeeRate100Percent)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == expectedThisAccountToExistErrorMsg)
  })

  it("Verifies you can't Deposit Over the Global Limit", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.depositTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, accountName)
      .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == globalLimitExceededErrorMsg)
  })

  it("Verifies That Only the CEO Can Update the Token Reserve", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.updateTokenReserve(solTokenMintAddress, borrowAPY7Percent, true, globalLimit1, solvencyInsurancefeeRate4Percent)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notCEOErrorMsg)
  })

  it("Updates Token Reserve Borrow APY, Global Limit, and Solvency Insurance Rate", async () => 
  {
    await program.methods.updateTokenReserve(solTokenMintAddress, borrowAPY7Percent, true, globalLimit2, solvencyInsurancefeeRate7Percent).rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.borrowApy == borrowAPY7Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit2))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsurancefeeRate7Percent)
  })

  it("Deposits wSOL Into the Token Reserve", async () => 
  {
    await program.methods.depositTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, accountName)
    .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
   
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(twoSol))
  
    const tokenReserveATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
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
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(twoSol))
    assert(lendingUserMonthlyStatementAccount.monthlyDepositedAmount.eq(twoSol))

    //Populate sol remaining account
    const successorSOLLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierSOLLendingUserTabRemainingAccount = 
    {
      pubkey: successorSOLLendingUserTabAccountPDA,
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
      .accounts({ signer: successorWalletKeypair.publicKey })
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
    .accounts({ signer: successorWalletKeypair.publicKey })
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
      await program.methods.withdrawTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tooMuchSol, false)
      .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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
      await program.methods.withdrawTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, true)
      .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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
    const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount]

    await program.methods.withdrawTokens(
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      twoSol,
      true
    )
    .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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

    const tokenReserveATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == 0)

    var errorMessage = ""

    const userATA = await deriveATA(successorWalletKeypair.publicKey, solTokenMintAddress, true)
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
      program.provider.publicKey,
      testSubMarketIndex,
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
    await program.methods.addTokenReserve(usdcMint.publicKey, usdcTokenDecimalAmount, usdcPythFeedIDArray, borrowAPY5Percent, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsurancefeeRate4Percent)//IDE complains about ByteArray but still works
    .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.pythFeedId.toString() == usdcPythFeedIDArray.toString())
    assert(tokenReserve.borrowApy == borrowAPY5Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit1))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsurancefeeRate4Percent)
  })

  it("Creates a USDC SubMarket", async () => 
  {
    await program.methods.createSubMarket(usdcMint.publicKey, testSubMarketIndex, program.provider.publicKey, subMarketFeeRate4Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate4Percent)
    assert(subMarket.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  it("Deposits USDC Into the Token Reserve", async () => 
  {
    await program.methods.depositTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, supplierUSDCAmount, null)
    .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
   
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(supplierUSDCAmount))

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
    assert(lendingUserTabAccount.depositedAmount.eq(supplierUSDCAmount))

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(supplierUSDCAmount))
    assert(lendingUserMonthlyStatementAccount.monthlyDepositedAmount.eq(supplierUSDCAmount))

    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)
    assert(parseInt(tokenReserveUSDCATABalance.value.amount) == supplierUSDCAmount.toNumber())

    //Populate USDC remaining account
    const usdcLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )

    supplierUSDCLendingUserTabRemainingAccount = 
    {
      pubkey: usdcLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }
  })

  it("Deposits 1 SOL as Collateral", async () => 
  {
    //Depositing 1 Sol as Collateral
    await program.methods.depositTokens(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, oneSol, accountName)
    .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .rpc()

    //Populate Borrower SOL remaining account
    const borrowerSOLLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    )
    borrowerSOLLendingUserTabRemainingAccount = 
    {
      pubkey: borrowerSOLLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Borrower USDC remaining account
    const borrowerUSDCLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    )
    borrowerUSDCLendingUserTabRemainingAccount = 
    {
      pubkey: borrowerUSDCLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }
  })

  it("Verifies that you can't Borrow More than 70% of the Value of your Collateral", async () => 
  {
    var errorMessage = ""

    try
    {
      const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, borrowerUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]
      await program.methods.borrowTokens(
        usdcMint.publicKey,
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        overBorrowUSDCAmount
      )
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([borrowerWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == debtExceeding70PercentOfCollateralErrorMsg)
  })

  it("Borrows USDC From the Token Reserve", async () => 
  {
    //Borrowing from the USDC that the Successor deposited
    const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, borrowerUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]
    await program.methods.borrowTokens(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      borrowerUSDCAmount
    )
    .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts(remainingAccounts)
    .signers([borrowerWalletKeypair])
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    console.log("Token Reserve Supply Interest Change Index: ", Number(tokenReserve.supplyInterestChangeIndex))
    console.log("Token Reserve Borrow Interest Change Index: ", Number(tokenReserve.borrowInterestChangeIndex))
    assert(tokenReserve.borrowedAmount.eq(borrowerUSDCAmount))
    assert(tokenReserve.supplyApy == tokenReserve.borrowApy * tokenReserve.utilizationRate / 10000)
    assert(tokenReserve.utilizationRate == Number(tokenReserve.borrowedAmount) / Number(tokenReserve.depositedAmount) * 10000)
    
    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))

    assert(lendingUserTabAccount.borrowedAmount.eq(borrowerUSDCAmount))
  })

  it("Verifies that you can't Withdraw an Amount that Would Cause Your Debt to be More than 70% of the Value of your Collateral", async () => 
  {
    var errorMessage = ""

    try
    {
      const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, borrowerUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]
      await program.methods.withdrawTokens(
        solTokenMintAddress,
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        new anchor.BN(1),
        false
      )
      .accounts({ mint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([borrowerWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == debtExceeding70PercentOfCollateralErrorMsg)
  })

  it("Verifies you can't Withdraw When too many Tokens are Currently Being Borrowed.", async () => 
  {
    //Allow some time after borrow for interest to increase
    await timeOutFunction(borrowWaitTimeInSeconds)

    var errorMessage = ""

    try
    {
      const remainingAccounts = [supplierUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount, supplierSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount]
      
      await program.methods.withdrawTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount, true)
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == insufficientLiquidityErrorMsg)
  })

  it("Verifies you can't Borrow When too many Tokens are Currently Being Borrowed.", async () => 
  {
    var errorMessage = ""

    try
    {
      const remainingAccounts = [supplierUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount, supplierSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount]
      
      await program.methods.borrowTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount)
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == insufficientLiquidityErrorMsg)
  })

  it("Verifies you can't liquidate an Account whose Debt Value is 80% or less than its Collateral Value", async () => 
  {
    var errorMessage = ""

    try
    {
      //Update Price timestamp for SOL Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        solPythPriceUpdateAccountKeypair,
        solPythFeedIDBuffer,
        solTestPrice,
        solTestConf,
        pythPriceDecimals
      )

      //Update Price timestamp for USDC Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        usdcPythPriceUpdateAccountKeypair,
        usdcPythFeedIDBuffer,
        usdcTestPrice,
        usdcTestConf,
        pythPriceDecimals
      )

      const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, borrowerUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]

      await program.methods.liquidateAccount(
        usdcMint.publicKey,
        solTokenMintAddress,
        program.provider.publicKey,
        testSubMarketIndex,
        program.provider.publicKey,
        testSubMarketIndex,
        borrowerWalletKeypair.publicKey,
        testUserAccountIndex,
        testUserAccountIndex,
        halfBorrowerUSDCAmount,
        null
      )
      .accounts({ repaymentMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
      .remainingAccounts(remainingAccounts)
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notLiquidatableErrorMsg)
  })

  it("Verifies you can't repay more than 50% of someone's debt when liquidating them", async () => 
  {
    var errorMessage = ""

    try
    {
      const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, borrowerUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]

      await program.methods.liquidateAccount(
        usdcMint.publicKey,
        solTokenMintAddress,
        program.provider.publicKey,
        testSubMarketIndex,
        program.provider.publicKey,
        testSubMarketIndex,
        borrowerWalletKeypair.publicKey,
        testUserAccountIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        null
      )
      .accounts({ repaymentMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
      .remainingAccounts(remainingAccounts)
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == overLiquidationErrorMsg)
  })

  it("Liquidates Account whose Debt Value is more than 80% of their Collateral Value", async () => 
  {
    //Update Price timestamp for SOL Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      solPythPriceUpdateAccountKeypair,
      solPythFeedIDBuffer,
      solLiquidationPrice,
      solTestConf,
      pythPriceDecimals
    )

    //Update Price timestamp for USDC Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      usdcPythPriceUpdateAccountKeypair,
      usdcPythFeedIDBuffer,
      usdcTestPrice,
      usdcTestConf,
      pythPriceDecimals
    )

    const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, borrowerUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]

    await program.methods.liquidateAccount(
      usdcMint.publicKey,
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex,
      testUserAccountIndex,
      halfBorrowerUSDCAmount,
      null
    )
    .accounts({ repaymentMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .remainingAccounts(remainingAccounts)
    .rpc()

    //assert(errorMessage == notLiquidatableErrorMsg)
  })

  it("Verifies you can't Over Repay.", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.repayTokens(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      overBorrowUSDCAmount,
      false
      )
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == tooManyFundsErrorMsg)
  })

  it("Repays Borrowed USDC To the Token Reserve", async () => 
  {
    var tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    var tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    var tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)
    assert(tokenReserveUSDCATABalance.value.uiAmount == Number(tokenReserve.depositedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount) - 
    Number(tokenReserve.borrowedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount))

    await program.methods.repayTokens(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      borrowerUSDCAmount,
      true
    )
    .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .rpc()

    var tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.borrowedAmount.eq(bnZero))

    const borrowerLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(borrowerLendingUserTabAccount.borrowedAmount.eq(bnZero))

    var tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    var tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)

    const interestAccruedAmount = Number(borrowerLendingUserTabAccount.interestAccruedAmount) / Math.pow(10, tokenReserveUSDCATABalance.value.decimals)
    assert(tokenReserveUSDCATABalance.value.uiAmount == Number(supplierUSDCAmount) / Math.pow(10, tokenReserveUSDCATABalance.value.decimals) + interestAccruedAmount)
  })

  it("Verifies you Must Pass in the User Tab Accounts in the Order They Were Created", async () => 
  {
    var errorMessage = ""

    try
    {
      const remainingAccounts = [supplierUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount, supplierSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount]
      
      await program.methods.withdrawTokens(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount, true)
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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

  it("Updates SnapShot", async () => 
  {
    //Update Supplier SOL Tab SnapShot
    await program.methods.updateUserSnapShot(
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    .accounts({ signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    //Update Supplier USDC Tab SnapShot
    await program.methods.updateUserSnapShot(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    .accounts({ signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    //Update Borrower SOL Tab SnapShot so they can withdraw in the end
    await program.methods.updateUserSnapShot(
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    )
    .accounts({ signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .rpc()

    //Update Borrower USDC Tab SnapShot so they can withdraw in the end
    await program.methods.updateUserSnapShot(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    )
    .accounts({ signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .rpc()

    const borrowerLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(borrowerLendingUserTabAccount.borrowedAmount.eq(bnZero))

    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)

    const supplierLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    console.log("Token Reserve Supply Interest Change Index: ", Number(tokenReserve.supplyInterestChangeIndex))
    console.log("Token Reserve Borrow Interest Change Index: ", Number(tokenReserve.borrowInterestChangeIndex))
    console.log("Token Reserve Interest Earned: ", Number(tokenReserve.interestEarnedAmount))
    console.log("Token Reserve Interest Accrued: ", Number(tokenReserve.interestAccruedAmount))
    console.log("Token Reserve SubMarketFees: ", Number(tokenReserve.subMarketFeesGeneratedAmount))
    console.log("Token Reserve SolvencyFees: ", Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount))
    console.log("Token Reserve Balance After Repayment: ", tokenReserveUSDCATABalance.value.uiAmount)

    console.log("Supplier Interest Accrued: ", Number(supplierLendingUserTabAccount.interestAccruedAmount))
    console.log("Supplier Interest Earned: ", Number(supplierLendingUserTabAccount.interestEarnedAmount))
    console.log("Supplier SubMarket Fees Generated: ", Number(supplierLendingUserTabAccount.subMarketFeesGeneratedAmount))
    console.log("Supplier Solvency Insurance Generated: ", Number(supplierLendingUserTabAccount.solvencyInsuranceFeesGeneratedAmount))
    console.log("Borrower Interest Accrued: ", Number(borrowerLendingUserTabAccount.interestAccruedAmount))
    console.log("Borrower Interest Earned: ", Number(borrowerLendingUserTabAccount.interestEarnedAmount))
    console.log("Borrower SubMarket Fees Generated: ", Number(borrowerLendingUserTabAccount.subMarketFeesGeneratedAmount))
    console.log("Borrower Solvency Insurance Generated: ", Number(borrowerLendingUserTabAccount.solvencyInsuranceFeesGeneratedAmount))
  })

  it("Verifies that the Pyth Price Can't be Negative", async () => 
  {
    var errorMessage = ""

    try
    {
      //Update Price timestamp for SOL Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        solPythPriceUpdateAccountKeypair,
        solPythFeedIDBuffer,
        solNegativePrice,
        solTestConf,
        pythPriceDecimals
      )

      //Update Price timestamp for USDC Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        usdcPythPriceUpdateAccountKeypair,
        usdcPythFeedIDBuffer,
        usdcNegativePrice,
        usdcTestConf,
        pythPriceDecimals
      )

      //await debugPrintPythAccount(solPythPriceUpdateAccountKeypair.publicKey)
      //await debugPrintPythAccount(usdcPythPriceUpdateAccountKeypair.publicKey)
    
      const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, supplierUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]

      await program.methods.withdrawTokens(
        usdcMint.publicKey,
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        true
      )
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == negativePythPriceErrorMsg)
  })

  it("Verifies that the Pyth Price Confidence Must be Within 2%", async () => 
  {
    var errorMessage = ""

    try
    {
      //Update Price timestamp for SOL Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        solPythPriceUpdateAccountKeypair,
        solPythFeedIDBuffer,
        solTestPrice,
        solUncertainConf,
        pythPriceDecimals
      )

      //Update Price timestamp for USDC Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        usdcPythPriceUpdateAccountKeypair,
        usdcPythFeedIDBuffer,
        usdcTestPrice,
        usdcUncertainConf,
        pythPriceDecimals
      )

      //await debugPrintPythAccount(usdcPythPriceUpdateAccountKeypair.publicKey)
    
      const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, supplierUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]

      await program.methods.withdrawTokens(
        usdcMint.publicKey,
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        true
      )
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == unstablePythPriceErrorMsg)
  })

  it("Withdraws USDC From the Token Reserve", async () => 
  {
    //await debugPrintPythAccount(usdcPythPriceUpdateAccountKeypair.publicKey)

    //Update Price timestamp for SOL Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      solPythPriceUpdateAccountKeypair,
      solPythFeedIDBuffer,
      solTestPrice,
      solTestConf,
      pythPriceDecimals
    )

    //Update Price timestamp for USDC Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      usdcPythPriceUpdateAccountKeypair,
      usdcPythFeedIDBuffer,
      usdcTestPrice,
      usdcTestConf,
      pythPriceDecimals
    )

    //await debugPrintPythAccount(usdcPythPriceUpdateAccountKeypair.publicKey)
  
    const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, supplierUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]

    await program.methods.withdrawTokens(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      supplierUSDCAmount,
      true
    )
    .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .remainingAccounts(remainingAccounts)
    .signers([successorWalletKeypair])
    .rpc()

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

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    console.log("Token Reserve Interest Earned: ", Number(tokenReserve.interestEarnedAmount))
    console.log("Token Reserve Interest Accrued: ", Number(tokenReserve.interestAccruedAmount))
    console.log("Token Reserve SubMarketFees: ", Number(tokenReserve.subMarketFeesGeneratedAmount))
    console.log("Token Reserve SolvencyFees: ", Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount))
    console.log("Token Reserve Deposited Amount After User Withdrawal: ", Number(tokenReserve.depositedAmount))

    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)
    console.log("Token Reserve Balance After Withdrawal: ", parseInt(tokenReserveUSDCATABalance.value.amount))
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(parseInt(tokenReserveUSDCATABalance.value.amount) == Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount) + Number(subMarket.uncollectedSubMarketFeesAmount))

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(bnZero))
    const withDrawAmount = supplierUSDCAmount.add(lendingUserMonthlyStatementAccount.monthlyInterestEarnedAmount)
    assert(lendingUserMonthlyStatementAccount.monthlyWithdrawalAmount.eq(withDrawAmount))

    const userATA = await deriveATA(successorWalletKeypair.publicKey, usdcMint.publicKey, true)
    const UserATAAccount = await program.provider.connection.getTokenAccountBalance(userATA)
    assert(parseInt(UserATAAccount.value.amount) == tenKUSDC + Number(lendingUserMonthlyStatementAccount.monthlyInterestEarnedAmount))
  })

  it("Verifies only Fee Collector can Collect Fees from Submarket", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.claimSubMarketFees(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      null
      )
      .accounts({ signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notFeeCollectorErrorMsg)
  })

  it("Claims SubMarket Fees", async () => 
  {
    await program.methods.claimSubMarketFees(
    usdcMint.publicKey,
    program.provider.publicKey,
    testSubMarketIndex,
    testUserAccountIndex,
    null
    )
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      program.provider.publicKey,
      testUserAccountIndex
    ))
    
    //Claiming SubMarket Fees just puts it in the Fee Collector's Tab Account
    assert(parseInt(tokenReserveUSDCATABalance.value.amount) == Number(lendingUserTabAccount.depositedAmount) + Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount))

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(subMarket.uncollectedSubMarketFeesAmount.eq(bnZero))
  })

  it("Verifies only Treasurer can Collect Solvency Insurance Fees", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.claimSolvencyInsuranceFees(
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      null
      )
      .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notTreasurerErrorMsg)
  })

  it("Claims Token Reserve Solvency Insurance Fees", async () => 
  {
    await program.methods.claimSolvencyInsuranceFees(
    usdcMint.publicKey,
    program.provider.publicKey,
    testSubMarketIndex,
    testUserAccountIndex,
    null
    )
    .accounts({ mint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      program.provider.publicKey,
      testUserAccountIndex
    ))

    assert(parseInt(tokenReserveUSDCATABalance.value.amount) == Number(lendingUserTabAccount.depositedAmount))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.uncollectedSolvencyInsuranceFeesAmount.eq(bnZero))
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

  function getlendingUserMonthlyStatementAccountPDA(statementMonth: number,
    statementYear: number,
    tokenMintAddress: PublicKey,
    subMarketOwnerAddress: PublicKey,
    subMarketIndex: number,
    lendingUserAddress: PublicKey,
    lendingUserAccountIndex: number)
  {
    const [lendingUserMonthlyStatementAccountPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("userMonthlyStatementAccount"),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
        new anchor.BN(statementMonth).toBuffer('le', 1),
        new anchor.BN(statementYear).toBuffer('le', 4),
        tokenMintAddress.toBuffer(),
        subMarketOwnerAddress.toBuffer(),
        new anchor.BN(subMarketIndex).toBuffer('le', 2),
        lendingUserAddress.toBuffer(),
        new anchor.BN(lendingUserAccountIndex).toBuffer('le', 1),
      ],
      program.programId
    )
    return lendingUserMonthlyStatementAccountPDA
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

  async function deriveATA(walletPublicKey: PublicKey, tokenMintAddress: PublicKey, pdaAccount: boolean = false)
  {
    if(tokenMintAddress.toString() == solTokenMintAddress.toString())
      return await Token.getAssociatedTokenAddress
      (
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        tokenMintAddress,
        walletPublicKey,
        pdaAccount
      )
    else
      return await Token.getAssociatedTokenAddress
      (
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_2022_PROGRAM_ID,
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
        TOKEN_2022_PROGRAM_ID,
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
        TOKEN_2022_PROGRAM_ID,
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
  feedId: Buffer<ArrayBuffer>,
  price: bigint,
  conf: anchor.BN,
  exponent: number)
  {
    //Get latest block chain timestamp.
    const slot = await program.provider.connection.getSlot()
    const timeStamp = await program.provider.connection.getBlockTime(slot)

    const publish_time = new anchor.BN(timeStamp)
    const prev_publish_time = new anchor.BN(timeStamp - 1)

    // Allocate a 136-byte buffer.
    const buf = Buffer.alloc(mockedPythAccountSpace)
    let offset = 0
    
    //1. Write the 8-byte Pyth Discriminator/Magic Number. (8 bytes)
    const discriminator = Buffer.from([34, 241, 35, 99, 157, 126, 244, 205])
    discriminator.copy(buf, offset)
    offset += 8 // offset = 8
    
    //2. Write the write_authority (32 bytes).
    const writeAuthority = PublicKey.unique().toBuffer()
    writeAuthority.copy(buf, offset)
    offset += 32 // offset = 40
    
    //3. Write verification_level (1 byte tag).
    buf.writeUInt8(1, offset) // tag '1' for Full verification (1 byte)
    offset += 1 // offset = 41
    
    //PriceFeedMessage starts here (Total 92 bytes):
    //4. feedID (32 bytes)
    feedId.copy(buf, offset)
    offset += 32 // offset = 76
    
    //6. price (i64, 8 bytes)
    buf.writeBigInt64LE(price, offset)
    //price.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    offset += 8 // offset = 84
    
    //7. conf (u64, 8 bytes)
    conf.toArrayLike(Buffer, "le", 8).copy(buf, offset)
    //buf.writeBigInt64LE(conf, offset)
    offset += 8 // offset = 92
    
    //8. exponent (i32, 4 bytes)
    buf.writeInt32LE(exponent, offset)
    offset += 4 // offset = 96
    
    //9. publish_time (i64, 8 bytes)
    publish_time.toArrayLike(Buffer, "le", 8).copy(buf, offset)
    offset += 8 // offset = 104
    
    //10. prev_publish_time (i64, 8 bytes)
    prev_publish_time.toArrayLike(Buffer, "le", 8).copy(buf, offset)
    offset += 8 // offset = 112
    
    //11. ema_price (i64, 8 bytes)
    //price.toArrayLike(Buffer, "le", 8).copy(buf, offset);
    buf.writeBigInt64LE(price, offset)
    offset += 8 // offset = 120
    
    //12. ema_conf (u64, 8 bytes)
    conf.toArrayLike(Buffer, "le", 8).copy(buf, offset)
    offset += 8; // offset = 128
    
    //13. posted_slot (u64, 8 bytes)
    (new anchor.BN(0)).toArrayLike(Buffer, "le", 8).copy(buf, offset)
    offset += 8 // offset = 136

    //Write the buffer data to the mock account
    await mockProgram.methods.setMockedPythPriceUpdateAccount(buf)
    .accounts({ mockedPythPriceUpdateAccount: mockedPythKeyPair.publicKey })
    .signers([mockedPythKeyPair])
    .rpc()
  }

  async function debugPrintPythAccount(accountPubkey: PublicKey)
  {
    const accountInfo = await program.provider.connection.getAccountInfo(accountPubkey)
    
    if (!accountInfo)
    {
      console.log("Account not found!")
      return
    }

    const data = accountInfo.data
    
    // Manual Parsing based on your buffer layout
    // Offset 0-8: Discriminator
    // Offset 8-40: Write Authority
    // Offset 40: Verification Level
    // Offset 41-73: Feed ID
    // Offset 73-81: Price
    // Offset 81-89: Conf
    // Offset 89-93: Exponent
    // Offset 93-101: Publish Time

    const feedId = data.subarray(41, 73).toString('hex')
    const price = data.readBigInt64LE(73)
    const conf = data.readBigUInt64LE(81)
    const exponent = data.readInt32LE(89)
    const publishTime = data.readBigInt64LE(93)
    
    console.log("--- DEBUG PYTH ACCOUNT ---")
    console.log("Feed ID (Hex):", feedId)
    console.log("Price:", price.toString())
    console.log("Price:", conf.toString())
    console.log("Exponent:", exponent)
    console.log("Publish Time:", publishTime.toString())
    
    // Check against current time
    const slot = await program.provider.connection.getSlot()
    const currentTime = await program.provider.connection.getBlockTime(slot)
    console.log("Current Chain Time:", currentTime)
    console.log("Age (seconds):", currentTime - Number(publishTime))
    console.log("--------------------------")
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

  async function timeOutFunction(timeToWaitInSeconds: number)
  {
    timeOutCountDown(timeToWaitInSeconds)

    const timeToWaitInMilliSeconds = timeToWaitInSeconds * 1000
    console.log("Sleeping for: " + timeToWaitInSeconds + " seconds")
    await sleep(timeToWaitInMilliSeconds)
  }

  function timeOutCountDown(timeToWaitInSeconds: number)
  {
    var timeLeftInSeconds = timeToWaitInSeconds
    console.log(`${timeLeftInSeconds} Seconds Left`)

    const countDownIntervalId = setInterval(() =>
    {
      timeLeftInSeconds -= 10
      if(timeLeftInSeconds > 0)
        console.log(`${timeLeftInSeconds} Seconds Left`)
      
      if(timeLeftInSeconds <= 0)
        clearInterval(countDownIntervalId)  
    }, 10000) 
  }
})