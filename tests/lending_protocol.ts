import * as anchor from "@coral-xyz/anchor"
import { Program } from "@coral-xyz/anchor"
import { LendingProtocol } from "../target/types/lending_protocol"
import { PythMock } from "../target/types/pyth_mock"
import { assert } from "chai"
import * as fs from 'fs'
import { PublicKey,
  LAMPORTS_PER_SOL,
  Transaction,
  Keypair,
  SystemProgram,
  VersionedTransaction,
  TransactionMessage,
  ComputeBudgetProgram,
  AddressLookupTableProgram
} from '@solana/web3.js'
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
  const pythPriceDecimalsTest = 9
  const notCEOErrorMsg = "Only the CEO can call this function"
  const notSolvencyTreasurerErrorMsg = "Only the Solvency Treasurer can call this function"
  const notLiquidationTreasurerErrorMsg = "Only the Liquidation Treasurer can call this function"
  const solvencyInsuranceFeeOnInterestEarnedRateTooHighMsg = "The solvency insurance fee on interest earned rate can't be greater than 100%"
  const subMarketFeeOnInterestEarnedRateTooHighMsg = "The submarket fee on interest earned rate can't be greater than 100%"
  const feeOnInterestEarnedRateTooLowMsg = "ERR_OUT_OF_RANGE"
  const globalLimitExceededErrorMsg = "You can't deposit more than the global limit"
  const expectedThisAccountToExistErrorMsg = "The program expected this account to be already initialized"
  const insufficientFundsErrorMsg = "You can't withdraw more funds than you've deposited"
  const ataDoesNotExistErrorMsg = "failed to get token account balance: Invalid param: could not find account"
  const debtExceeding70PercentOfCollateralErrorMsg = "You can't withdraw or borrow an amount that would cause your borrow liabilities to exceed 70% of deposited collateral"
  const insufficientLiquidityErrorMsg = "Not enough liquidity in the Token Reserve for this withdraw or borrow"
  const notLiquidatableErrorMsg = "You can't liquidate an account whose borrow liabilities aren't 80% or more of their deposited collateral"
  const overLiquidationErrorMsg = "You can't repay more than 50% of a liquidati's debt position"
  const notInsolventErrorMsg = "You can't zero out an account whose borrow liabilities aren't 100% or more of their deposited collateral"
  const tooManyFundsErrorMsg = "You can't pay back more funds than you've borrowed"
  const griefingErrorMsg = "You must repay atleast 10% of the borrow position if the account is in an unhealthy state. This prevents 'griefing'"
  const incorrectOrderOfTabAccountsErrorMsg = "You must provide the sub user's tab accounts ordered by user_tab_account_index"
  const accountNameTooLongErrorMsg = "Lending User Account name can't be longer than 25 characters"
  const negativePythPriceErrorMsg = "Negative Price Detected"
  const unstablePythPriceErrorMsg = "Oracle Price Too Unstable"
  const unexpectedTabAccountErrorMsg = "Unexpected Tab Account PDA detected. Feed in only legitimate PDA's ordered by user_tab_account_index"
  const unexpectedTokenReserveErrorMsg = "Unexpected Token Reserve Account PDA detected"
  const unexpectedSubMarketErrorMsg = "Unexpected SubMarket Account PDA detected"
  const unexpectedMonthlyStatementErrorMsg = "Unexpected Monthly Statement Account PDA detected"
  const unexpectedPythAccountFeedIDOrStalePriceErrorMsg = "The price data was stale or the feed id was incorrect"
  const notFeeCollectorErrorMsg = "Only the Fee Collector can claim the fees"
  const staleTokenReserveErrorMsg = "The token reserve was stale"
  
  var lookUpTableAddress: PublicKey
  var lookUpTableInstance: anchor.web3.TransactionInstruction
  var lookUpTableAccount: anchor.web3.AddressLookupTableAccount

  var lendingStatsRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  const solTokenMintAddress = new PublicKey("So11111111111111111111111111111111111111112")
  //const solTokenMintAddress = new PublicKey("9pan9bMn5HatX4EJdBwg9VgCa7Uz5HL8N1m5D3NdXejP")
  const solPythFeedIDBuffer = Buffer.from("ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d", "hex")
  const solPythFeedIDArray = Array.from(Buffer.from("ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d", "hex"))
  const solTokenDecimalAmount = 9
  const oneSol = new anchor.BN(LAMPORTS_PER_SOL)
  const twoSol = new anchor.BN(LAMPORTS_PER_SOL * 2)
  const solTestPrice = BigInt(10_000_000_000)//8 Decimal Price
  const solNegativePrice = BigInt(-10_000_000_000)//8 Decimal Price
  const solCantLiquidatePrice = BigInt(87_500_000_100)//9 Decimal Price for testing
  const solTestConf = new anchor.BN(245)
  const solUncertainConf = new anchor.BN(200_000_001)
  var solPythPriceUpdateAccountKeypair: Keypair
  var solTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var solSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierSOLLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerSOLLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierSOLMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerSOLMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var solPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  
  const mintAmount = 10_000_000_000

  var usdcMint = undefined
  const usdcPythFeedIDBuffer = Buffer.from("eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a", "hex")
  const usdcPythFeedIDArray = Array.from(Buffer.from("eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a", "hex"))
  const usdcTokenDecimalAmount = 6
  const halfBorrowerUSDCAmount = new anchor.BN(35_000_000)
  const borrowerUSDCAmount = new anchor.BN(70_000_000)
  const overBorrowUSDCAmount = new anchor.BN(71_000_000)
  const lessThan10PercentOfBorrowedAmount = new anchor.BN(6_999_999) 
  const supplierUSDCAmount = new anchor.BN(100_000_000)
  const usdcTestPrice = BigInt(100_000_000)//8 Decimal Price
  const usdcNegativePrice = BigInt(-100_000_000)//8 Decimal Price
  const usdcTestConf = new anchor.BN(245)
  const usdcUncertainConf = new anchor.BN(2_000_001)
  var usdcPythPriceUpdateAccountKeypair: Keypair
  var usdcTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var usdcSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierUSDCLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerUSDCLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierUSDCMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerUSDCMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var usdcPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  var daiMint = undefined
  const daiPythFeedIDBuffer = Buffer.from("b0948a5e5313200c632b51bb5ca32f6de0d36e9950a942d19751e833f70dabfd", "hex")
  const daiPythFeedIDArray = Array.from(Buffer.from("b0948a5e5313200c632b51bb5ca32f6de0d36e9950a942d19751e833f70dabfd", "hex"))
  const daiTokenDecimalAmount = 8
  const daiDepositAmount = new anchor.BN(10_000_000_000)
  const daiHalfDepositAmount = new anchor.BN(5_000_000_000)
  const daiTestPrice = BigInt(100_000_000)//8 Decimal Price
  const daiTestConf = new anchor.BN(245)
  var daiPythPriceUpdateAccountKeypair: Keypair
  var daiTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var daiSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierDAILendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerDAILendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierDAIMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerDAIMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var daiPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  var wethMint = undefined
  const wethPythFeedIDBuffer = Buffer.from("9d4294bbcd1174d6f2003ec365831e64cc31d9f6f15a2b85399db8d5000960f6", "hex")
  const wethPythFeedIDArray = Array.from(Buffer.from("9d4294bbcd1174d6f2003ec365831e64cc31d9f6f15a2b85399db8d5000960f6", "hex"))
  const wethTokenDecimalAmount = 8
  const wethDepositAmount = new anchor.BN(10_000_000_000)
  const wethHalfDepositAmount = new anchor.BN(5_000_000_000)
  const wethTestPrice = BigInt(100_000_000)//8 Decimal Price
  const wethTestConf = new anchor.BN(245)
  var wethPythPriceUpdateAccountKeypair: Keypair
  var wethTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var wethSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWEthLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWEthLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWEthMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWEthMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var wethPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  var wbtcMint = undefined
  const wbtcPythFeedIDBuffer = Buffer.from("c9d8b075a5c69303365ae23633d4e085199bf5c520a3b90fed1322a0342ffc33", "hex")
  const wbtcPythFeedIDArray = Array.from(Buffer.from("c9d8b075a5c69303365ae23633d4e085199bf5c520a3b90fed1322a0342ffc33", "hex"))
  const wbtcTokenDecimalAmount = 8
  const wbtcDepositAmount = new anchor.BN(10_000_000_000)
  const wbtcHalfDepositAmount = new anchor.BN(5_000_000_000)
  const wbtcTestPrice = BigInt(100_000_000)//8 Decimal Price
  const wbtcTestConf = new anchor.BN(245)
  var wbtcPythPriceUpdateAccountKeypair: Keypair
  var wbtcTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var wbtcSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWBtcLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWBtcLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWBtcMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWBtcMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var wbtcPythPriceUpdateRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  const borrowAPY5Percent = 500 //5.00%
  const borrowAPY7Percent = 700 //7.00%
  const globalLimitLow = new anchor.BN(1)
  const globalLimit1 = new anchor.BN(10_000_000_000)
  const globalLimit2 = new anchor.BN(20_000_000_000)

  const solvencyInsuranceFeeRateAbove100Percent = 10001 //100.01%
  const solvencyInsuranceFeeRateBelove0Percent = -1 //-0.01%
  const solvencyInsuranceFeeRate8Percent = 800 //8.00%
  const solvencyInsuranceFeeRate7Percent = 700 //7.00%

  const subMarketFeeRateAbove100Percent = 10001 //100.01%
  const subMarketFeeRateBelove0Percent = -1 //-0.01%
  const subMarketFeeRate8Percent = 800 //8%
  const subMarketFeeRate100Percent = 10000 //100.00%

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
  //const borrowWaitTimeInSeconds = 0
  const useUSDCFixedBorrowAPY = false
  const runInsolventTest = true
  var solLiquidationPrice: bigint

  if(!runInsolventTest)
    solLiquidationPrice = BigInt(87_500_000_000)//9 Decimal Price for testing
  else
    solLiquidationPrice = BigInt(70_000_000_000)//9 Decimal Price for testing

  before(async () =>
  {
    //Fund Successor Wallet
    console.log("Funding Sucessor Wallet")
    await airDropSol(successorWalletKeypair.publicKey)

    //Fund Borrower Wallet
    console.log("Funding Borrower Wallet")
    await airDropSol(borrowerWalletKeypair.publicKey)

    //Create a new USDC Mint for testing
    console.log("Creating Token Mints and ATAs for Testing")

    usdcMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      program.provider.publicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      usdcTokenDecimalAmount, //Decimals for USDC
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    daiMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      program.provider.publicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      daiTokenDecimalAmount, //Decimals for DAI
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    wethMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      program.provider.publicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      wethTokenDecimalAmount, //Decimals for WETH
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    wbtcMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      program.provider.publicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      wbtcTokenDecimalAmount, //Decimals for WBTC
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    //Mint USDC to CEO Wallet
    const testingWalletUSDCATA = await deriveATA(program.provider.publicKey, usdcMint.publicKey)
    await createATAForWallet(testingWalletKeypair, usdcMint.publicKey, testingWalletUSDCATA)
    await mintTokenToWallet(usdcMint.publicKey, testingWalletUSDCATA)

    //Mint USDC to Successor Wallet
    const successorWalletUSDCATA = await deriveATA(successorWalletKeypair.publicKey, usdcMint.publicKey)
    await createATAForWallet(successorWalletKeypair, usdcMint.publicKey, successorWalletUSDCATA)
    await mintTokenToWallet(usdcMint.publicKey, successorWalletUSDCATA)

    //Mint USDC to Borrower Wallet
    const borrowerWalletUSDCATA = await deriveATA(borrowerWalletKeypair.publicKey, usdcMint.publicKey)
    await createATAForWallet(borrowerWalletKeypair, usdcMint.publicKey, borrowerWalletUSDCATA)
    await mintTokenToWallet(usdcMint.publicKey, borrowerWalletUSDCATA)

    //Test other tokens
    //Mint DAI to Successor Wallet
    const successorWalletDAIATA = await deriveATA(successorWalletKeypair.publicKey, daiMint.publicKey)
    await createATAForWallet(successorWalletKeypair, daiMint.publicKey, successorWalletDAIATA)
    await mintTokenToWallet(daiMint.publicKey, successorWalletDAIATA)
    //Mint WETH to Successor Wallet
    const successorWalletWETHATA = await deriveATA(successorWalletKeypair.publicKey, wethMint.publicKey)
    await createATAForWallet(successorWalletKeypair, wethMint.publicKey, successorWalletWETHATA)
    await mintTokenToWallet(wethMint.publicKey, successorWalletWETHATA)
    //Mint WBTC to Successor Wallet
    const successorWalletWBTCATA = await deriveATA(successorWalletKeypair.publicKey, wbtcMint.publicKey)
    await createATAForWallet(successorWalletKeypair, wbtcMint.publicKey, successorWalletWBTCATA)
    await mintTokenToWallet(wbtcMint.publicKey, successorWalletWBTCATA)

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

    //Mock DAI Pyth Price Update Account
    console.log("Setting up DAI Mocked Pyth Price Update Account")

    daiPythPriceUpdateAccountKeypair = await createMockedPythPriceUpdateAccount()

    await updateMockedPriceUpdateV2Account
    (
      daiPythPriceUpdateAccountKeypair,
      daiPythFeedIDBuffer,
      daiTestPrice,
      daiTestConf,
      pythPriceDecimals
    )

    daiPythPriceUpdateRemainingAccount = 
    {
      pubkey: daiPythPriceUpdateAccountKeypair.publicKey,
      isSigner: false,
      isWritable: true
    }

    //Mock WEth Pyth Price Update Account
    console.log("Setting up WEth Mocked Pyth Price Update Account")

    wethPythPriceUpdateAccountKeypair = await createMockedPythPriceUpdateAccount()

    await updateMockedPriceUpdateV2Account
    (
      wethPythPriceUpdateAccountKeypair,
      wethPythFeedIDBuffer,
      wethTestPrice,
      wethTestConf,
      pythPriceDecimals
    )

    wethPythPriceUpdateRemainingAccount = 
    {
      pubkey: wethPythPriceUpdateAccountKeypair.publicKey,
      isSigner: false,
      isWritable: true
    }

    //Mock WBtc Pyth Price Update Account
    console.log("Setting up WBtc Mocked Pyth Price Update Account")

    wbtcPythPriceUpdateAccountKeypair = await createMockedPythPriceUpdateAccount()

    await updateMockedPriceUpdateV2Account
    (
      wbtcPythPriceUpdateAccountKeypair,
      wbtcPythFeedIDBuffer,
      wbtcTestPrice,
      wbtcTestConf,
      pythPriceDecimals
    )

    wbtcPythPriceUpdateRemainingAccount = 
    {
      pubkey: wbtcPythPriceUpdateAccountKeypair.publicKey,
      isSigner: false,
      isWritable: true
    }

    await initLookUpTable()

    console.log("Setup Complete")
  })

  it("Initializes Lending Protocol", async () => 
  {
    await program.methods.initializeLendingProtocol(statementMonth, statementYear).rpc()

    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOPDA())
    assert(ceoAccount.address.toBase58() == program.provider.publicKey.toBase58())

    const lendingProtocolPDA = getLendingProtocolPDA()
    var lendingProtocol = await program.account.lendingProtocol.fetch(lendingProtocolPDA)
    assert(lendingProtocol.currentStatementMonth == statementMonth)
    assert(lendingProtocol.currentStatementYear == statementYear)

    //Populate Lending Stats remaining account
    const lendingStatsPDA = getLendingStatsPDA()
    lendingStatsRemainingAccount = 
    {
      pubkey: lendingStatsPDA,
      isSigner: false,
      isWritable: true
    }

    //Add Lending Protocol and Lending Stats to Address Lookup Table
    await addAddressToLookUpTable([lendingProtocolPDA, lendingStatsPDA])

    //Get latest lookup table
    lookUpTableAccount = (await program.provider.connection.getAddressLookupTable(lookUpTableAddress)).value
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
    
    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOPDA())
    assert(ceoAccount.address.toBase58() == successorWalletKeypair.publicKey.toBase58())
  })
  
  it("Passes back the Lending Protocol CEO Account", async () => 
  {
    await program.methods.passOnLendingProtocolCeo(program.provider.publicKey)
    .accounts({ signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOPDA())
    assert(ceoAccount.address.toBase58() == program.provider.publicKey.toBase58())
  })

  it("Verifies That Only the Solvency Treasurer Can Pass On Account", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.passOnSolvencyTreasurer(program.provider.publicKey)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notSolvencyTreasurerErrorMsg)
  })

  it("Passes on the Solvency Treasurer Account", async () => 
  {
    await program.methods.passOnSolvencyTreasurer(successorWalletKeypair.publicKey).rpc()

    var treasurerAccount = await program.account.solvencyTreasurer.fetch(getSolvencyTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == successorWalletKeypair.publicKey.toBase58())
  })
  
  it("Passes back the Solvency Treasurer Account", async () => 
  {
    await program.methods.passOnSolvencyTreasurer(program.provider.publicKey)
    .accounts({ signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    var treasurerAccount = await program.account.solvencyTreasurer.fetch(getSolvencyTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == program.provider.publicKey.toBase58())
  })

  it("Verifies That Only the Liquidation Treasurer Can Pass On Account", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.passOnLiquidationTreasurer(program.provider.publicKey)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notLiquidationTreasurerErrorMsg)
  })

  it("Passes on the Liquidation Treasurer Account", async () => 
  {
    await program.methods.passOnLiquidationTreasurer(successorWalletKeypair.publicKey).rpc()

    var treasurerAccount = await program.account.liquidationTreasurer.fetch(getLiquidationTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == successorWalletKeypair.publicKey.toBase58())
  })
  
  it("Passes back the Liquidation Treasurer Account", async () => 
  {
    await program.methods.passOnLiquidationTreasurer(program.provider.publicKey)
    .accounts({ signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    var treasurerAccount = await program.account.liquidationTreasurer.fetch(getLiquidationTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == program.provider.publicKey.toBase58())
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

  it("Updates Lending Protocol Statement Month and Year", async () => 
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
      await program.methods.addTokenReserve(solTokenDecimalAmount, solPythFeedIDArray, borrowAPY5Percent, true, globalLimit1, solvencyInsuranceFeeRate8Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notCEOErrorMsg)
  })

  it("Verifies That a Token Reserve Can't be Created With a Solvency Insurance Fee on Interest Rate Higher than 100%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.addTokenReserve(solTokenDecimalAmount, solPythFeedIDArray, borrowAPY5Percent, true, globalLimitLow, solvencyInsuranceFeeRateAbove100Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == solvencyInsuranceFeeOnInterestEarnedRateTooHighMsg)
  })

  it("Verifies That a Token Reserve Can't be Created With a Solvency Insurance Fee on Interest Rate Below 0%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.addTokenReserve(solTokenDecimalAmount, solPythFeedIDArray, borrowAPY5Percent, true, globalLimitLow, solvencyInsuranceFeeRateBelove0Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.code
    }

    assert(errorMessage == feeOnInterestEarnedRateTooLowMsg)
  })
  
  it("Adds a wSOL Token Reserve", async () => 
  {
    await program.methods.addTokenReserve(solTokenDecimalAmount, solPythFeedIDArray, borrowAPY5Percent, true, globalLimitLow, solvencyInsuranceFeeRate8Percent)
    .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenReserveProtocolIndex == 0)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.pythFeedId.toString() == solPythFeedIDArray.toString())
    assert(tokenReserve.borrowApy == borrowAPY5Percent)
    assert(tokenReserve.globalLimit.eq(globalLimitLow))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate8Percent)

    //Populate SOL Token Reserve remaining account
    const solTokenReservePDA = getTokenReservePDA(solTokenMintAddress)
    solTokenReserveRemainingAccount = 
    {
      pubkey: solTokenReservePDA,
      isSigner: false,
      isWritable: true
    }

    //Add Token Reserve to Address Lookup Table
    await addAddressToLookUpTable(solTokenReservePDA)

    //Get latest lookup table
    lookUpTableAccount = (await program.provider.connection.getAddressLookupTable(lookUpTableAddress)).value
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

    assert(errorMessage == subMarketFeeOnInterestEarnedRateTooHighMsg)
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
    await program.methods.createSubMarket(solTokenMintAddress, testSubMarketIndex, program.provider.publicKey, subMarketFeeRate8Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(subMarket.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)

    //Populate SOL Sub Market Remaining Account
    const solSubMarketPDA = getSubMarketPDA(solTokenMintAddress, program.provider.publicKey, testSubMarketIndex)
    solSubMarketRemainingAccount = 
    {
      pubkey: solSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Add Sub Market to Address Lookup Table
    await addAddressToLookUpTable(solSubMarketPDA)

    //Get latest lookup table
    lookUpTableAccount = (await program.provider.connection.getAddressLookupTable(lookUpTableAddress)).value
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
      await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, accountName)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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
      await program.methods.updateTokenReserve(solTokenMintAddress, borrowAPY7Percent, true, globalLimit1, solvencyInsuranceFeeRate8Percent)
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
    await program.methods.updateTokenReserve(solTokenMintAddress, borrowAPY7Percent, true, globalLimit2, solvencyInsuranceFeeRate7Percent).rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.borrowApy == borrowAPY7Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit2))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate7Percent)
  })

  it("Deposits wSOL Into the Token Reserve", async () => 
  {
    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, accountName)
    .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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

    //Populate Supplier SOL Tab Remaining Account
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

    //Populate Supplier SOL Monthly Statement Remaining Account
    const supplierSOLMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierSOLMonthlyStatementRemainingAccount = 
    {
      pubkey: supplierSOLMonthlyStatementPDA,
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

  it("Verifies a User Can't Withdraw When the Token Reserve is Stale", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.withdrawTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, true)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == staleTokenReserveErrorMsg)
  })

  it("Verifies a User Can't Withdraw More wSOL Than They Deposited", async () => 
  {
    var errorMessage = ""
    const tooMuchSol = twoSol.add(new anchor.BN(1))

    try
    {
      const transaction = new Transaction()

      const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .instruction()

      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const withdrawInstruction = await program.methods.withdrawTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, tooMuchSol, false)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .instruction()

      transaction.add(withdrawInstruction)

      await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(insufficientFundsErrorMsg))
  })

  it("Withdraws wSOL From the Token Reserve", async () => 
  {
    const transaction = new Transaction()

    const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      supplierSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
    .accounts({ signer: successorWalletKeypair.publicKey })
    .remainingAccounts(remainingAccounts)
    .signers([successorWalletKeypair])
    .instruction()

    transaction.add(refreshUserHealthAndTokenReservesInstruction)

    const withdrawInstruction = await program.methods.withdrawTokens(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      twoSol,
      true
    )
    .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .instruction()

    transaction.add(withdrawInstruction)

    await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])

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

    var userBalance = await program.provider.connection.getBalance(successorWalletKeypair.publicKey)

    assert(userBalance >= 9999)

    //Verify that wrapped SOL ATA for User was closed since it was empty
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
    
    assert(errorMessage == ataDoesNotExistErrorMsg)
  })
  
  it("Adds a USDC Token Reserve", async () => 
  {
    await program.methods.addTokenReserve(usdcTokenDecimalAmount, usdcPythFeedIDArray, borrowAPY5Percent, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate8Percent)
    .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenReserveProtocolIndex == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.pythFeedId.toString() == usdcPythFeedIDArray.toString())
    assert(tokenReserve.borrowApy == borrowAPY5Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit1))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate8Percent)

    //Populate USDC Token Reserve remaining account
    const usdcTokenReservePDA = getTokenReservePDA(usdcMint.publicKey)
    usdcTokenReserveRemainingAccount = 
    {
      pubkey: usdcTokenReservePDA,
      isSigner: false,
      isWritable: true
    }

    //Add Token Reserve to Address Lookup Table
    await addAddressToLookUpTable(usdcTokenReservePDA)

    //Get latest lookup table
    lookUpTableAccount = (await program.provider.connection.getAddressLookupTable(lookUpTableAddress)).value
  })

  it("Creates a USDC SubMarket", async () => 
  {
    await program.methods.createSubMarket(usdcMint.publicKey, testSubMarketIndex, program.provider.publicKey, subMarketFeeRate8Percent).rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(subMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(subMarket.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(subMarket.subMarketIndex == testSubMarketIndex)

    //Populate USDC Sub Market Remaining Account
    const usdcSubMarketPDA = getSubMarketPDA(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex)
    usdcSubMarketRemainingAccount = 
    {
      pubkey: usdcSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Add Sub Market to Address Lookup Table
    await addAddressToLookUpTable(usdcSubMarketPDA)

    //Get latest lookup table
    lookUpTableAccount = (await program.provider.connection.getAddressLookupTable(lookUpTableAddress)).value
  })

  it("Deposits USDC Into the Token Reserve", async () => 
  {
    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, supplierUSDCAmount, null)
    .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
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

    //Populate Supplier USDC Tab Remaining Account
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

    //Populate Supplier SOL Monthly Statement Remaining Account
    const supplierUSDCMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierUSDCMonthlyStatementRemainingAccount = 
    {
      pubkey: supplierUSDCMonthlyStatementPDA,
      isSigner: false,
      isWritable: true
    }
  })

  it("Deposits 1 SOL as Collateral", async () => 
  {
    //Depositing 1 Sol as Collateral
    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, oneSol, accountName)
    .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .rpc()

    //Populate Borrower SOL Tab Remaining Account
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

    //Populate Borrower USDC Tab Remaining Account
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
    
    //Populate Borrower SOL Monthly Statement Remaining Account
    const borrowerSOLMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    )
    borrowerSOLMonthlyStatementRemainingAccount = 
    {
      pubkey: borrowerSOLMonthlyStatementPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Borrower USDC Monthly Statement Remaining Account
    const borrowerUSDCMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    )
    borrowerUSDCMonthlyStatementRemainingAccount = 
    {
      pubkey: borrowerUSDCMonthlyStatementPDA,
      isSigner: false,
      isWritable: true
    }
  })

  it("Verifies you Can't Refresh User's Health Without Their Tab Account", async () => 
  {
    var errorMessage = ""

    try
    {
      //Refresh Token Reserve and User Health
      const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == unexpectedTabAccountErrorMsg)
  })

  it("Verifies you Can't Refresh User's Health Without the Right Token Reserve", async () => 
  {
    var errorMessage = ""

    try
    {
      //Refresh Token Reserve and User Health
      const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == unexpectedTokenReserveErrorMsg)
  })

  it("Verifies you Can't Refresh User's Health Without the Right Sub Market", async () => 
  {
    var errorMessage = ""

    try
    {
      //Refresh Token Reserve and User Health
      const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == unexpectedSubMarketErrorMsg)
  })

  it("Verifies you Can't Refresh User's Health Without the Right Monthly Statement", async () => 
  {
    var errorMessage = ""

    try
    {
      //Refresh Token Reserve and User Health
      const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == unexpectedMonthlyStatementErrorMsg)
  })

  it("Verifies you Can't Refresh User's Health Without the Right Pyth Price Update Account", async () => 
  {
    var errorMessage = ""

    try
    {
      //Refresh Token Reserve and User Health
      const remainingAccounts = [supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == unexpectedPythAccountFeedIDOrStalePriceErrorMsg)
  })

  it("Verifies a User Can't Borrow When the Token Reserve is Stale", async () => 
  {
    var errorMessage = ""

    try
    {
      const remainingAccounts = [borrowerSOLLendingUserTabRemainingAccount, solPythPriceUpdateRemainingAccount, borrowerUSDCLendingUserTabRemainingAccount, usdcPythPriceUpdateRemainingAccount]
      await program.methods.borrowTokens(
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        overBorrowUSDCAmount
      )
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([borrowerWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == staleTokenReserveErrorMsg)
  })

  it("Verifies that you can't Borrow More than 70% of the Value of your Collateral", async () => 
  {
    var errorMessage = ""

    try
    {
      const transaction = new Transaction()

      const refreshingRemainingAccounts =
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([borrowerWalletKeypair])
      .instruction()

      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      //The user is borrowing from a Token Reserve they have never interacted with before in this case
      const refreshTokenReserveInstruction = await program.methods.refreshTokenReserveOnly(usdcMint.publicKey)
      .accounts({ signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .instruction()

      transaction.add(refreshTokenReserveInstruction)

      const borrowRemainingAccounts = [usdcPythPriceUpdateRemainingAccount]
      const borrowInstruction = await program.methods.borrowTokens(
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        overBorrowUSDCAmount
      )
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(borrowRemainingAccounts)
      .signers([borrowerWalletKeypair])
      .instruction()

      transaction.add(borrowInstruction)

      await program.provider.sendAndConfirm(transaction, [borrowerWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(debtExceeding70PercentOfCollateralErrorMsg))
  })

  it("Borrows USDC From the Token Reserve", async () => 
  {
    //Borrowing from the USDC that the Successor deposited
    const transaction = new Transaction()

    const refreshingRemainingAccounts = 
    [
      borrowerSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
    .accounts({ signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts(refreshingRemainingAccounts)
    .signers([borrowerWalletKeypair])
    .instruction()

    transaction.add(refreshUserHealthAndTokenReservesInstruction)

    //The user is borrowing from a Token Reserve they have never interacted with before in this case
    const refreshTokenReserveInstruction = await program.methods.refreshTokenReserveOnly(usdcMint.publicKey)
    .accounts({ signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .instruction()

    transaction.add(refreshTokenReserveInstruction)

    const borrowRemainingAccounts = [usdcPythPriceUpdateRemainingAccount]
    const borrowInstruction = await program.methods.borrowTokens(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      borrowerUSDCAmount
    )
    .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts(borrowRemainingAccounts)
    .signers([borrowerWalletKeypair])
    .instruction()

    transaction.add(borrowInstruction)

    await program.provider.sendAndConfirm(transaction, [borrowerWalletKeypair])

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
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([borrowerWalletKeypair])
      .instruction()

      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const withdrawRemainingAccounts = [solPythPriceUpdateRemainingAccount]
      const withdrawInstruction = await program.methods.withdrawTokens(
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        new anchor.BN(1),
        false
      )
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(withdrawRemainingAccounts)
      .signers([borrowerWalletKeypair])
      .instruction()

      transaction.add(withdrawInstruction)

      await program.provider.sendAndConfirm(transaction, [borrowerWalletKeypair])
    }
    catch(error)
    {
      //For some reason, simulation was passing, so the error was being returned at confirmation with no transactions logs
      if(error.transactionLogs)
      {
        errorMessage = error.transactionLogs.toString()
      }
      else
      {
        const errorInstance = error//The string wasn't presisting unless I did this or console.log(error.message) for some reason
        const errorString = errorInstance.message
        const errorMatch = errorString.match(/"Custom":(\d+)/)
        console.log(errorMatch)
        console.log(errorMatch[1])
        const idlError = program.idl.errors.find(error => error.code === parseInt(errorMatch[1]))
        errorMessage = idlError.msg
      }
    }

    assert(errorMessage == debtExceeding70PercentOfCollateralErrorMsg)

    //Allow some time after borrow for interest to increase
    //This was placed here and not the previous borrow test to allow this test to pass. Can't have interest already being earned, increasing the withdrawable amount.
    await timeOutFunction(borrowWaitTimeInSeconds)

    //Update Price timestamp for SOL Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      solPythPriceUpdateAccountKeypair,
      solPythFeedIDBuffer,
      solCantLiquidatePrice,  //Value where the SOL isn't Liquidatable
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
  })

  it("Verifies you can't Withdraw When too many Tokens are Currently Being Borrowed.", async () => 
  {
    var errorMessage = ""

    try
    {
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        supplierUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([successorWalletKeypair])
      .instruction()

      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const withdrawRemainingAccounts = [usdcPythPriceUpdateRemainingAccount]
      const withdrawInstruction = await program.methods.withdrawTokens(
        program.provider.publicKey,
        testSubMarketIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        true
      )
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(withdrawRemainingAccounts)
      .signers([successorWalletKeypair])
      .instruction()

      transaction.add(withdrawInstruction)

      await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(insufficientLiquidityErrorMsg))
  })

  it("Verifies you can't Borrow When too many Tokens are Currently Being Borrowed.", async () => 
  {
    var errorMessage = ""

    try
    {
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        supplierUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([successorWalletKeypair])
      .instruction()

      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const borrowRemainingAccounts = [usdcPythPriceUpdateRemainingAccount]
      
      const borrowInstruction = await program.methods.borrowTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount)
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(borrowRemainingAccounts)
      .signers([successorWalletKeypair])
      .instruction()

      transaction.add(borrowInstruction)

      await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(insufficientLiquidityErrorMsg))
  })

  it("Verifies you can't liquidate an Account whose Debt Value is less than 80% of its Collateral Value", async () => 
  {
    var errorMessage = ""

    try
    {
      //Update Price timestamp for SOL Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        solPythPriceUpdateAccountKeypair,
        solPythFeedIDBuffer,
        solCantLiquidatePrice,  //Value where the SOL isn't Liquidatable
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

      //const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      /*await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .remainingAccounts(refreshingRemainingAccounts)
      .rpc()*/

      //transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const liquidationRemainingAccounts =
      [
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        program.provider.publicKey,
        testSubMarketIndex,
        program.provider.publicKey,
        testSubMarketIndex,
        borrowerWalletKeypair.publicKey,
        testUserAccountIndex,
        testUserAccountIndex,
        halfBorrowerUSDCAmount,
        true,
        false,
        false,
        null
      )
      .accounts({ repaymentMint: usdcMint.publicKey, liquidationMint: solTokenMintAddress, repaymentTokenProgram: TOKEN_2022_PROGRAM_ID, liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      /*await program.methods.liquidateAccount(
        program.provider.publicKey,
        testSubMarketIndex,
        program.provider.publicKey,
        testSubMarketIndex,
        borrowerWalletKeypair.publicKey,
        testUserAccountIndex,
        testUserAccountIndex,
        halfBorrowerUSDCAmount,
        true,
        false,
        false,
        null
      )
      .accounts({ repaymentMint: usdcMint.publicKey, liquidationMint: solTokenMintAddress, repaymentTokenProgram: TOKEN_2022_PROGRAM_ID, liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .rpc()*/

      //transaction.add(liquidateInstruction)

      const { blockhash } = await program.provider.connection.getLatestBlockhash()

      //2. Compile your message (this converts your instructions into a Versioned format)
      const messageV0 = new TransactionMessage({
        payerKey: program.provider.publicKey,
        recentBlockhash: blockhash,
        instructions: [refreshUserHealthAndTokenReservesInstruction, liquidateInstruction]
      }).compileToV0Message([lookUpTableAccount])

      // Create and sign the Versioned Transaction
      const transaction = new VersionedTransaction(messageV0)
      //await program.provider.wallet.signTransaction(transaction)

      const size = transaction.serialize().length
      console.log(`Current Transaction Size: ${size} bytes`)

      const txid = await program.provider.sendAndConfirm(transaction)

      //const modifyComputeUnits = ComputeBudgetProgram.setComputeUnitLimit({ units: 300_000 })
      //transaction.add(modifyComputeUnits)

      //await program.provider.sendAndConfirm(transaction)
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(notLiquidatableErrorMsg))
  })

  it("Verifies a liquidator can't zero out an Account whose Debt Value is less than 100% of its Collateral Value", async () => 
  {
    var errorMessage = ""

    try
    {
      const refreshingRemainingAccounts = 
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      const liquidationRemainingAccounts =
      [
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        program.provider.publicKey,
        testSubMarketIndex,
        program.provider.publicKey,
        testSubMarketIndex,
        borrowerWalletKeypair.publicKey,
        testUserAccountIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        false,
        true,
        false,
        null
      )
      .accounts({ repaymentMint: usdcMint.publicKey, liquidationMint: solTokenMintAddress, repaymentTokenProgram: TOKEN_2022_PROGRAM_ID, liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      const { blockhash } = await program.provider.connection.getLatestBlockhash()

      const messageV0 = new TransactionMessage(
      {
        payerKey: program.provider.publicKey,
        recentBlockhash: blockhash,
        instructions: [refreshUserHealthAndTokenReservesInstruction, liquidateInstruction]
      }).compileToV0Message([lookUpTableAccount])

      //Create Versioned Transaction
      const transaction = new VersionedTransaction(messageV0)
      await program.provider.wallet.signTransaction(transaction)

      const size = transaction.serialize().length
      console.log(`Current Transaction Size: ${size} bytes`)

      await program.provider.sendAndConfirm(transaction)
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(notInsolventErrorMsg))
  })

  it("Verifies a liquidator can't repay more than 50% of someone's debt when liquidating an account that is in a bad state but not completely insolvent", async () => 
  {
    var errorMessage = ""

    try
    {
      //Update Price timestamp for SOL Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        solPythPriceUpdateAccountKeypair,
        solPythFeedIDBuffer,
        solLiquidationPrice, //Value where the SOL is Liquidatable
        solTestConf,
        pythPriceDecimalsTest
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

      const refreshingRemainingAccounts = 
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      const liquidationRemainingAccounts =
      [
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        program.provider.publicKey,
        testSubMarketIndex,
        program.provider.publicKey,
        testSubMarketIndex,
        borrowerWalletKeypair.publicKey,
        testUserAccountIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        false,
        false,
        false,
        null
      )
      .accounts({ repaymentMint: usdcMint.publicKey, liquidationMint: solTokenMintAddress, repaymentTokenProgram: TOKEN_2022_PROGRAM_ID, liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      const { blockhash } = await program.provider.connection.getLatestBlockhash()

      const messageV0 = new TransactionMessage(
      {
        payerKey: program.provider.publicKey,
        recentBlockhash: blockhash,
        instructions: [refreshUserHealthAndTokenReservesInstruction, liquidateInstruction]
      }).compileToV0Message([lookUpTableAccount])

      //Create Versioned Transaction
      const transaction = new VersionedTransaction(messageV0)
      await program.provider.wallet.signTransaction(transaction)

      const size = transaction.serialize().length
      console.log(`Current Transaction Size: ${size} bytes`)

      await program.provider.sendAndConfirm(transaction)
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(overLiquidationErrorMsg))
  })

  it("Verifies a Borrower can't Repay less than 10% when their account is in a bad state to prevent 'griefing'.", async () => 
  {
    var errorMessage = ""

    try
    {
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .signers([borrowerWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const repayTokenInstruction = await program.methods.repayTokens(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      lessThan10PercentOfBorrowedAmount,
      false
      )
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .instruction()

      transaction.add(repayTokenInstruction)

      await program.provider.sendAndConfirm(transaction, [borrowerWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(griefingErrorMsg))
  })

  it("Verifies a liquidator can't repay less than 10% of an account in a bad state to prevent 'griefing'", async () => 
  {
    var errorMessage = ""

    try
    {
      //Update Price timestamp for SOL Pyth mocked account
      await updateMockedPriceUpdateV2Account
      (
        solPythPriceUpdateAccountKeypair,
        solPythFeedIDBuffer,
        solLiquidationPrice, //Value where the SOL is Liquidatable
        solTestConf,
        pythPriceDecimalsTest
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

      const refreshingRemainingAccounts = 
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      const liquidationRemainingAccounts =
      [
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        program.provider.publicKey,
        testSubMarketIndex,
        program.provider.publicKey,
        testSubMarketIndex,
        borrowerWalletKeypair.publicKey,
        testUserAccountIndex,
        testUserAccountIndex,
        lessThan10PercentOfBorrowedAmount,
        false,
        false,
        false,
        null
      )
      .accounts({ repaymentMint: usdcMint.publicKey, liquidationMint: solTokenMintAddress, repaymentTokenProgram: TOKEN_2022_PROGRAM_ID, liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      const { blockhash } = await program.provider.connection.getLatestBlockhash()

      const messageV0 = new TransactionMessage(
      {
        payerKey: program.provider.publicKey,
        recentBlockhash: blockhash,
        instructions: [refreshUserHealthAndTokenReservesInstruction, liquidateInstruction]
      }).compileToV0Message([lookUpTableAccount])

      //Create Versioned Transaction
      const transaction = new VersionedTransaction(messageV0)
      await program.provider.wallet.signTransaction(transaction)

      const size = transaction.serialize().length
      console.log(`Current Transaction Size: ${size} bytes`)

      await program.provider.sendAndConfirm(transaction)
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(griefingErrorMsg))
  })

  //Liquidation test type controlled by "runInsolventTest" variable
  it("Liquidates or Zero's out insolvent Account whose Debt Value is 100% or more of their Collateral Value", async () => 
  {
    console.log("\n", "<-- Before Liquidation -->")

    var lendingStats = await program.account.lendingStats.fetch(getLendingStatsPDA())
    console.log("Liquidations: ", lendingStats.liquidations)

    var repaymentTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    var repaymentTokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    var repaymentTokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(repaymentTokenReserveUSDCATA)
    console.log("Repayment Token Reserve Deposited Amount Before Liquidation", Number(repaymentTokenReserve.depositedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Borrowed Amount Before Liquidation", Number(repaymentTokenReserve.borrowedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Repaid Debt Before Liquidation", Number(repaymentTokenReserve.repaidDebtAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Wallet Balance Before Liquidation", repaymentTokenReserveUSDCATABalance.value.uiAmount, "USDC", "\n")

    var liquidationTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    var liquidationTokenReserveUSDCATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    var liquidationTokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(liquidationTokenReserveUSDCATA)
    console.log("Liquidation Token Reserve Deposited Amount Before Liquidation", Number(liquidationTokenReserve.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Liquidated Amount Before Liquidation", Number(liquidationTokenReserve.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Liquidation Fees Generated Amount Before Liquidation", Number(liquidationTokenReserve.liquidationFeesGeneratedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Uncollected Liquidation Fee Amopunt Before Liquidation", Number(liquidationTokenReserve.uncollectedLiquidationFeesAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Wallet Balance Before Liquidation", liquidationTokenReserveUSDCATABalance.value.uiAmount, "SOL", "\n")

    var liquidatiRepaymentLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Borrowed Amount Before Liquidation", Number(liquidatiRepaymentLendingUserTabAccount.borrowedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")

    var liquidatiLiquidationLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Deposited Amount Before Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidati Liquidated Amount Before Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")

    const refreshingRemainingAccounts = 
    [
      borrowerSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount,

      borrowerUSDCLendingUserTabRemainingAccount,
      usdcTokenReserveRemainingAccount,
      usdcSubMarketRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount,
      usdcPythPriceUpdateRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
    .remainingAccounts(refreshingRemainingAccounts)
    .instruction()

    const liquidationRemainingAccounts =
    [
      lendingStatsRemainingAccount,
      usdcSubMarketRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerUSDCLendingUserTabRemainingAccount,
      borrowerSOLLendingUserTabRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,
      usdcPythPriceUpdateRemainingAccount,
      solPythPriceUpdateRemainingAccount
    ]

    const liquidateInstruction = await program.methods.liquidateAccount(
      program.provider.publicKey,
      testSubMarketIndex,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex,
      testUserAccountIndex,
      halfBorrowerUSDCAmount,
      true,
      runInsolventTest,
      false,
      null
    )
    .accounts({ repaymentMint: usdcMint.publicKey, liquidationMint: solTokenMintAddress, repaymentTokenProgram: TOKEN_2022_PROGRAM_ID, liquidationTokenProgram: TOKEN_PROGRAM_ID })
    .remainingAccounts(liquidationRemainingAccounts)
    .instruction()

    const { blockhash } = await program.provider.connection.getLatestBlockhash()

    const messageV0 = new TransactionMessage(
    {
      payerKey: program.provider.publicKey,
      recentBlockhash: blockhash,
      instructions: [refreshUserHealthAndTokenReservesInstruction, liquidateInstruction]
    }).compileToV0Message([lookUpTableAccount])

    //Create Versioned Transaction
    const transaction = new VersionedTransaction(messageV0)
    //await program.provider.wallet.signTransaction(transaction)

    await program.provider.sendAndConfirm(transaction)

    //4. Simulate using the non-deprecated config object
    /*const simulation = await program.provider.connection.simulateTransaction(versionedTransaction,
    {
      replaceRecentBlockhash: true,
      commitment: 'processed',
    })

    //5. Extract Compute Units
    const unitsConsumed = simulation.value.unitsConsumed * 1.5
    console.log("Estimated Compute Units:", unitsConsumed)

    const modifyComputeUnits = ComputeBudgetProgram.setComputeUnitLimit({ units: unitsConsumed })

    const transaction = new anchor.web3.Transaction()
      .add(modifyComputeUnits)
      .add(liquidateInstruction)

    await program.provider.sendAndConfirm(transaction)

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
    .rpc()*/

    console.log("<-- After Liquidation -->")

    var repaymentTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    var repaymentTokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    var repaymentTokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(repaymentTokenReserveUSDCATA)
    console.log("Repayment Token Reserve Deposited Amount After Liquidation", Number(repaymentTokenReserve.depositedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Borrowed Amount After Liquidation", Number(repaymentTokenReserve.borrowedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Repaid Debt After Liquidation", Number(repaymentTokenReserve.repaidDebtAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Wallet Balance After Liquidation", repaymentTokenReserveUSDCATABalance.value.uiAmount, "USDC", "\n")

    var liquidationTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    var liquidationTokenReserveSOLATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    var liquidationTokenReserveSOLATABalance = await program.provider.connection.getTokenAccountBalance(liquidationTokenReserveSOLATA)
    console.log("Liquidation Token Reserve Deposited Amount After Liquidation", Number(liquidationTokenReserve.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Liquidated Amount After Liquidation", Number(liquidationTokenReserve.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Liquidation Fees Generated Amount After Liquidation", Number(liquidationTokenReserve.liquidationFeesGeneratedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Uncollected Liquidation Fee Amopunt After Liquidation", Number(liquidationTokenReserve.uncollectedLiquidationFeesAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Wallet Balance After Liquidation", liquidationTokenReserveSOLATABalance.value.uiAmount, "SOL", "\n")

    const liquidatorLendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      program.provider.publicKey,
      testUserAccountIndex
    ))
    assert(liquidatorLendingUserAccount.accountName == "Generic Liquidator")
    assert(liquidatorLendingUserAccount.tabAccountCount == 2)

    const liquidatorLiquidationLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      program.provider.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidator Liquidation Amount After Liquidation", Number(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidator Solvency Fee Generated Amount After Liquidation", Number(liquidatorLiquidationLendingUserTabAccount.liquidationFeesGeneratedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")
    assert(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount.gt(bnZero))
    assert(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount.eq(liquidatorLiquidationLendingUserTabAccount.depositedAmount))

    var liquidatiRepaymentLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Borrowed Amount After Liquidation", Number(liquidatiRepaymentLendingUserTabAccount.borrowedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    assert(liquidatiRepaymentLendingUserTabAccount.borrowedAmount.eq(repaymentTokenReserve.borrowedAmount))

    var liquidatiLiquidationLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Deposited Amount After Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidati Liquidated Amount After Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    assert(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount.eq(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount.add(liquidatorLiquidationLendingUserTabAccount.liquidationFeesGeneratedAmount)))
    assert(oneSol.eq(liquidatiLiquidationLendingUserTabAccount.depositedAmount.add(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount)))

    const liquidatiRepaymentMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati SnapShot Debt Balance Amount After Liquidation", Number(liquidatiRepaymentMonthlyStatementAccount.snapShotDebtAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    assert(liquidatiRepaymentMonthlyStatementAccount.snapShotDebtAmount.eq(liquidatiRepaymentLendingUserTabAccount.borrowedAmount))
    assert(liquidatiRepaymentMonthlyStatementAccount.snapShotRepaidDebtAmount.eq(liquidatiRepaymentLendingUserTabAccount.repaidDebtAmount))
    
    const liquidatiLiquidationMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Monthly Statement Liquidated Amount After Liquidation", Number(liquidatiLiquidationMonthlyStatementAccount.monthlyLiquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidati SnapShot Deposit Balance Amount After Liquidation", Number(liquidatiLiquidationMonthlyStatementAccount.snapShotBalanceAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidati SnapShot Liquidated Amount After Liquidation", Number(liquidatiLiquidationMonthlyStatementAccount.snapShotLiquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")
    assert(liquidatiLiquidationMonthlyStatementAccount.snapShotBalanceAmount.eq(oneSol.sub(liquidatiLiquidationMonthlyStatementAccount.monthlyLiquidatedAmount)))

    const liquidatorLiquidationMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      program.provider.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidator Monthly Statement Liquidated Amount After Liquidation", Number(liquidatorLiquidationMonthlyStatementAccount.monthlyLiquidatorAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidator SnapShot Deposit Balance Amount After Liquidation", Number(liquidatorLiquidationMonthlyStatementAccount.snapShotBalanceAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidator SnapShot Liquidator Amount After Liquidation", Number(liquidatorLiquidationMonthlyStatementAccount.snapShotLiquidatorAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")
    assert(liquidatorLiquidationMonthlyStatementAccount.monthlyLiquidatorAmount.eq(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount))
    assert(liquidatorLiquidationMonthlyStatementAccount.snapShotBalanceAmount.eq(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount))
    assert(liquidatorLiquidationMonthlyStatementAccount.snapShotLiquidatorAmount.eq(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount))
  })

  it("", async () => 
  {
    //Refresh Token Reserve and User Health
    
  })
 
  it("Refreshes Supplier's and Borrower's Health Status", async () => 
  {
    //Refresh Supplier
    const refreshingSupplierRemainingAccounts = 
    [
      supplierSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      supplierSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount,

      supplierUSDCLendingUserTabRemainingAccount,
      usdcTokenReserveRemainingAccount,
      usdcSubMarketRemainingAccount,
      supplierUSDCMonthlyStatementRemainingAccount,
      usdcPythPriceUpdateRemainingAccount
    ]

    await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
    .accounts({ signer: successorWalletKeypair.publicKey })
    .remainingAccounts(refreshingSupplierRemainingAccounts)
    .signers([successorWalletKeypair])
    .rpc()

    //Refresh Borrower
    const refreshingBorrowerRemainingAccounts = 
    [
      borrowerSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount,

      borrowerUSDCLendingUserTabRemainingAccount,
      usdcTokenReserveRemainingAccount,
      usdcSubMarketRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount,
      usdcPythPriceUpdateRemainingAccount
    ]

    await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
    .accounts({ signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts(refreshingBorrowerRemainingAccounts)
    .signers([borrowerWalletKeypair])
    .rpc()

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

    const borrowerLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    console.log("Token Reserve Supply Interest Change Index: ", Number(tokenReserve.supplyInterestChangeIndex))
    console.log("Token Reserve Borrow Interest Change Index: ", Number(tokenReserve.borrowInterestChangeIndex))
    console.log("Token Reserve Interest Earned: ", Number(tokenReserve.interestEarnedAmount))
    console.log("Token Reserve Interest Accrued: ", Number(tokenReserve.interestAccruedAmount))
    console.log("Token Reserve SubMarketFees: ", Number(tokenReserve.subMarketFeesGeneratedAmount))
    console.log("Token Reserve SolvencyFees: ", Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount))
    console.log("Token Reserve Balance After Repayment: ", tokenReserveUSDCATABalance.value.uiAmount, "\n")

    console.log("Supplier Interest Earned: ", Number(supplierLendingUserTabAccount.interestEarnedAmount))
    console.log("Supplier Interest Accrued: ", Number(supplierLendingUserTabAccount.interestAccruedAmount))
    console.log("Supplier SubMarket Fees Generated: ", Number(supplierLendingUserTabAccount.subMarketFeesGeneratedAmount))
    console.log("Supplier Solvency Insurance Generated: ", Number(supplierLendingUserTabAccount.solvencyInsuranceFeesGeneratedAmount), "\n")

    console.log("Borrower Interest Earned: ", Number(borrowerLendingUserTabAccount.interestEarnedAmount))
    console.log("Borrower Interest Accrued: ", Number(borrowerLendingUserTabAccount.interestAccruedAmount))
    console.log("Borrower SubMarket Fees Generated: ", Number(borrowerLendingUserTabAccount.subMarketFeesGeneratedAmount))
    console.log("Borrower Solvency Insurance Generated: ", Number(borrowerLendingUserTabAccount.solvencyInsuranceFeesGeneratedAmount), "\n")
  })

  it("Verifies you can't Over Repay.", async () => 
  {
    var errorMessage = ""

    try
    {
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        borrowerSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
      .signers([borrowerWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const repayTokenInstruction = await program.methods.repayTokens(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      overBorrowUSDCAmount,
      false
      )
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .instruction()

      transaction.add(repayTokenInstruction)

      await program.provider.sendAndConfirm(transaction, [borrowerWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(tooManyFundsErrorMsg))
  })

  it("Repays Borrowed USDC To the Token Reserve", async () => 
  {
    var tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    var tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    var tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)
    var currentTokenReserveAmount = Number((Number(tokenReserve.depositedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount) -
    Number(tokenReserve.borrowedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount) +
    Number(tokenReserve.subMarketFeesGeneratedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount) +
    Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount)).toFixed(tokenReserve.tokenDecimalAmount))
    assert(tokenReserveUSDCATABalance.value.uiAmount >= currentTokenReserveAmount)

    const transaction = new Transaction()

    const refreshingRemainingAccounts = 
    [
      borrowerSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount,

      borrowerUSDCLendingUserTabRemainingAccount,
      usdcTokenReserveRemainingAccount,
      usdcSubMarketRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount,
      usdcPythPriceUpdateRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(borrowerWalletKeypair.publicKey, testUserAccountIndex)
    .signers([borrowerWalletKeypair])
    .remainingAccounts(refreshingRemainingAccounts)
    .instruction()
    
    transaction.add(refreshUserHealthAndTokenReservesInstruction)

    const repayTokenInstruction = await program.methods.repayTokens(
    program.provider.publicKey,
    testSubMarketIndex,
    testUserAccountIndex,
    borrowerUSDCAmount,
    true
    )
    .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .instruction()

    transaction.add(repayTokenInstruction)

    await program.provider.sendAndConfirm(transaction, [borrowerWalletKeypair])

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
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        supplierUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount,

        supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .signers([successorWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const withdrawInstruction = await program.methods.withdrawTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount, true)
        .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
        .signers([successorWalletKeypair])
        .instruction()

      transaction.add(withdrawInstruction)

      await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])

    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(incorrectOrderOfTabAccountsErrorMsg))
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
    
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        supplierUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .signers([successorWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const withdrawInstruction = await program.methods.withdrawTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount, true)
        .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
        .signers([successorWalletKeypair])
        .instruction()

      transaction.add(withdrawInstruction)

      await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(negativePythPriceErrorMsg))
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
    
      const transaction = new Transaction()

      const refreshingRemainingAccounts = 
      [
        supplierSOLLendingUserTabRemainingAccount,
        solTokenReserveRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,
        solPythPriceUpdateRemainingAccount,

        supplierUSDCLendingUserTabRemainingAccount,
        usdcTokenReserveRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount,
        usdcPythPriceUpdateRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
      .signers([successorWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      transaction.add(refreshUserHealthAndTokenReservesInstruction)

      const withdrawInstruction = await program.methods.withdrawTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount, true)
        .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
        .signers([successorWalletKeypair])
        .instruction()

      transaction.add(withdrawInstruction)

      await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])
    }
    catch(error)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(unstablePythPriceErrorMsg))
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
  
    const transaction = new Transaction()

    const refreshingRemainingAccounts = 
    [
      supplierSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      supplierSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount,

      supplierUSDCLendingUserTabRemainingAccount,
      usdcTokenReserveRemainingAccount,
      usdcSubMarketRemainingAccount,
      supplierUSDCMonthlyStatementRemainingAccount,
      usdcPythPriceUpdateRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
    .signers([successorWalletKeypair])
    .remainingAccounts(refreshingRemainingAccounts)
    .instruction()
    
    transaction.add(refreshUserHealthAndTokenReservesInstruction)

    const withdrawInstruction = await program.methods.withdrawTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, borrowerUSDCAmount, true)
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .instruction()

    transaction.add(withdrawInstruction)

    await program.provider.sendAndConfirm(transaction, [successorWalletKeypair])

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
    assert(parseInt(tokenReserveUSDCATABalance.value.amount) >= Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount) + Number(subMarket.uncollectedSubMarketFeesAmount))

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
    assert(parseInt(UserATAAccount.value.amount) == mintAmount + Number(lendingUserMonthlyStatementAccount.monthlyInterestEarnedAmount))
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
    assert(parseInt(tokenReserveUSDCATABalance.value.amount) >= Number(lendingUserTabAccount.depositedAmount) + Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount))

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(subMarket.uncollectedSubMarketFeesAmount.eq(bnZero))
  })

  it("Verifies only Solvency Treasurer can Collect Solvency Insurance Fees", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.claimSolvencyInsuranceFees(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      null
      )
      .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == notSolvencyTreasurerErrorMsg)
  })

  it("Claims Token Reserve Solvency Insurance Fees", async () => 
  {
    await program.methods.claimSolvencyInsuranceFees(
    program.provider.publicKey,
    testSubMarketIndex,
    testUserAccountIndex,
    null
    )
    .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
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

    assert(parseInt(tokenReserveUSDCATABalance.value.amount) >= Number(lendingUserTabAccount.depositedAmount))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.uncollectedSolvencyInsuranceFeesAmount.eq(bnZero))
  })

  it("Verifies only Liquidation Treasurer can Collect Liquidation Fees", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.claimLiquidationFees(
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

    assert(errorMessage == notLiquidationTreasurerErrorMsg)
  })

  it("Claims Token Reserve Liquidation Fees", async () => 
  {
    await program.methods.claimLiquidationFees(
    solTokenMintAddress,
    program.provider.publicKey,
    testSubMarketIndex,
    testUserAccountIndex,
    null
    )
    .rpc()

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTokenMintAddress,
      program.provider.publicKey,
      testSubMarketIndex,
      program.provider.publicKey,
      testUserAccountIndex
    ))

    assert(lendingUserTabAccount.liquidationFeesGeneratedAmount.gt(bnZero))
    assert(lendingUserTabAccount.liquidationFeesGeneratedAmount.eq(lendingUserTabAccount.liquidationFeesCollectedAmount))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.uncollectedLiquidationFeesAmount.eq(bnZero))
  })

  it("Adds a DAI, WEth, and WBtc Token Reserves", async () => 
  {
    await program.methods.addTokenReserve(daiTokenDecimalAmount, daiPythFeedIDArray, borrowAPY5Percent, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate8Percent)
    .accounts({ tokenMint: daiMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    await program.methods.addTokenReserve(wethTokenDecimalAmount, wethPythFeedIDArray, borrowAPY5Percent, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate8Percent)
    .accounts({ tokenMint: wethMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    await program.methods.addTokenReserve(wbtcTokenDecimalAmount, wbtcPythFeedIDArray, borrowAPY5Percent, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate8Percent)
    .accounts({ tokenMint: wbtcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    const daiTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(daiMint.publicKey))
    assert(daiTokenReserve.tokenReserveProtocolIndex == 2)
    assert(daiTokenReserve.tokenMintAddress.toBase58() == daiMint.publicKey.toBase58())
    assert(daiTokenReserve.tokenDecimalAmount == daiTokenDecimalAmount)
    assert(daiTokenReserve.depositedAmount.eq(bnZero))
    assert(daiTokenReserve.pythFeedId.toString() == daiPythFeedIDArray.toString())
    assert(daiTokenReserve.borrowApy == borrowAPY5Percent)
    assert(daiTokenReserve.globalLimit.eq(globalLimit1))
    assert(daiTokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate8Percent)

    const wethTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(wethMint.publicKey))
    assert(wethTokenReserve.tokenReserveProtocolIndex == 3)
    assert(wethTokenReserve.tokenMintAddress.toBase58() == wethMint.publicKey.toBase58())
    assert(wethTokenReserve.tokenDecimalAmount == wethTokenDecimalAmount)
    assert(wethTokenReserve.depositedAmount.eq(bnZero))
    assert(wethTokenReserve.pythFeedId.toString() == wethPythFeedIDArray.toString())
    assert(wethTokenReserve.borrowApy == borrowAPY5Percent)
    assert(wethTokenReserve.globalLimit.eq(globalLimit1))
    assert(wethTokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate8Percent)

    const wbtcTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(wbtcMint.publicKey))
    assert(wbtcTokenReserve.tokenReserveProtocolIndex == 4)
    assert(wbtcTokenReserve.tokenMintAddress.toBase58() == wbtcMint.publicKey.toBase58())
    assert(wbtcTokenReserve.tokenDecimalAmount == wbtcTokenDecimalAmount)
    assert(wbtcTokenReserve.depositedAmount.eq(bnZero))
    assert(wbtcTokenReserve.pythFeedId.toString() == wbtcPythFeedIDArray.toString())
    assert(wbtcTokenReserve.borrowApy == borrowAPY5Percent)
    assert(wbtcTokenReserve.globalLimit.eq(globalLimit1))
    assert(wbtcTokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate8Percent)

    //Populate DAI Token Reserve remaining account
    const daiTokenReservePDA = getTokenReservePDA(daiMint.publicKey)
    daiTokenReserveRemainingAccount = 
    {
      pubkey: daiTokenReservePDA,
      isSigner: false,
      isWritable: true
    }

    //Populate WEth Token Reserve remaining account
    const wethTokenReservePDA = getTokenReservePDA(wethMint.publicKey)
    wethTokenReserveRemainingAccount = 
    {
      pubkey: wethTokenReservePDA,
      isSigner: false,
      isWritable: true
    }

    //Populate WBtc Token Reserve remaining account
    const wbtcTokenReservePDA = getTokenReservePDA(wbtcMint.publicKey)
    wbtcTokenReserveRemainingAccount = 
    {
      pubkey: wbtcTokenReservePDA,
      isSigner: false,
      isWritable: true
    }

    //Add Token Reserves to Address Lookup Table
    await addAddressToLookUpTable(daiTokenReservePDA)
    await addAddressToLookUpTable(wethTokenReservePDA)
    await addAddressToLookUpTable(wbtcTokenReservePDA)

    //Get latest lookup table
    lookUpTableAccount = (await program.provider.connection.getAddressLookupTable(lookUpTableAddress)).value
  })

  it("Creates a DAI, WEth, and WBtc SubMarket", async () => 
  {
    await program.methods.createSubMarket(daiMint.publicKey, testSubMarketIndex, program.provider.publicKey, subMarketFeeRate8Percent).rpc()
    await program.methods.createSubMarket(wethMint.publicKey, testSubMarketIndex, program.provider.publicKey, subMarketFeeRate8Percent).rpc()
    await program.methods.createSubMarket(wbtcMint.publicKey, testSubMarketIndex, program.provider.publicKey, subMarketFeeRate8Percent).rpc()

    const daiSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(daiMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(daiSubMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(daiSubMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(daiSubMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(daiSubMarket.tokenMintAddress.toBase58() == daiMint.publicKey.toBase58())
    assert(daiSubMarket.subMarketIndex == testSubMarketIndex)

    const wethSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(wethMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(wethSubMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(wethSubMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(wethSubMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(wethSubMarket.tokenMintAddress.toBase58() == wethMint.publicKey.toBase58())
    assert(wethSubMarket.subMarketIndex == testSubMarketIndex)

    const wbtcSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(wbtcMint.publicKey, program.provider.publicKey, testSubMarketIndex))
    assert(wbtcSubMarket.owner.toBase58() == program.provider.publicKey.toBase58())
    assert(wbtcSubMarket.feeCollectorAddress.toBase58() == program.provider.publicKey.toBase58())
    assert(wbtcSubMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(wbtcSubMarket.tokenMintAddress.toBase58() == wbtcMint.publicKey.toBase58())
    assert(wbtcSubMarket.subMarketIndex == testSubMarketIndex)

    //Populate DAI Sub Market Remaining Account
    const daiSubMarketPDA = getSubMarketPDA(daiMint.publicKey, program.provider.publicKey, testSubMarketIndex)
    daiSubMarketRemainingAccount = 
    {
      pubkey: daiSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate WEth Sub Market Remaining Account
    const wethSubMarketPDA = getSubMarketPDA(wethMint.publicKey, program.provider.publicKey, testSubMarketIndex)
    wethSubMarketRemainingAccount = 
    {
      pubkey: wethSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate WBtc Sub Market Remaining Account
    const wbtcSubMarketPDA = getSubMarketPDA(wbtcMint.publicKey, program.provider.publicKey, testSubMarketIndex)
    wbtcSubMarketRemainingAccount = 
    {
      pubkey: wbtcSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Add Sub Markets to Address Lookup Table
    await addAddressToLookUpTable(daiSubMarketPDA)
    await addAddressToLookUpTable(wethSubMarketPDA)
    await addAddressToLookUpTable(wbtcSubMarketPDA)

    //Get latest lookup table
    lookUpTableAccount = (await program.provider.connection.getAddressLookupTable(lookUpTableAddress)).value
  })

  it("Deposits SOL, USDC, DAI, WEth, BTC into Token Reserve", async () => 
  {
    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, twoSol, accountName)
    .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, supplierUSDCAmount, null)
    .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, daiDepositAmount, null)
    .accounts({ tokenMint: daiMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, wethDepositAmount, null)
    .accounts({ tokenMint: wethMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    await program.methods.depositTokens(program.provider.publicKey, testSubMarketIndex, testUserAccountIndex, wbtcDepositAmount, null)
    .accounts({ tokenMint: wbtcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    //Populate Supplier DAI Tab Remaining Account
    const successorDAILendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      daiMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierDAILendingUserTabRemainingAccount = 
    {
      pubkey: successorDAILendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Supplier WEth Tab Remaining Account
    const successorWEthLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      wethMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierWEthLendingUserTabRemainingAccount = 
    {
      pubkey: successorWEthLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Supplier WBtc Tab Remaining Account
    const successorWBtcLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      wbtcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierWBtcLendingUserTabRemainingAccount = 
    {
      pubkey: successorWBtcLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Supplier DAI Monthly Statement Remaining Account
    const supplierDAIMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      daiMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierDAIMonthlyStatementRemainingAccount = 
    {
      pubkey: supplierDAIMonthlyStatementPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Supplier WEth Monthly Statement Remaining Account
    const supplierWEthMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      wethMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierWEthMonthlyStatementRemainingAccount = 
    {
      pubkey: supplierWEthMonthlyStatementPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Supplier WBtc Monthly Statement Remaining Account
    const supplierWBtcMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      wbtcMint.publicKey,
      program.provider.publicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    supplierWBtcMonthlyStatementRemainingAccount = 
    {
      pubkey: supplierWBtcMonthlyStatementPDA,
      isSigner: false,
      isWritable: true
    }
  })

  it("Withdraws DAI, WEth, and WBtc From the Token Reserve", async () => 
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

    //Update Price timestamp for DAI Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      daiPythPriceUpdateAccountKeypair,
      daiPythFeedIDBuffer,
      daiTestPrice,
      daiTestConf,
      pythPriceDecimals
    )

    //Update Price timestamp for WEth Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      wethPythPriceUpdateAccountKeypair,
      wethPythFeedIDBuffer,
      wethTestPrice,
      wethTestConf,
      pythPriceDecimals
    )

    //Update Price timestamp for WBtc Pyth mocked account
    await updateMockedPriceUpdateV2Account
    (
      wbtcPythPriceUpdateAccountKeypair,
      wbtcPythFeedIDBuffer,
      wbtcTestPrice,
      wbtcTestConf,
      pythPriceDecimals
    )

    //await debugPrintPythAccount(usdcPythPriceUpdateAccountKeypair.publicKey)
  
    const remainingAccounts =
    [ 
      supplierSOLLendingUserTabRemainingAccount,
      solPythPriceUpdateRemainingAccount,
      supplierUSDCLendingUserTabRemainingAccount,
      usdcPythPriceUpdateRemainingAccount,
      supplierDAILendingUserTabRemainingAccount,
      daiPythPriceUpdateRemainingAccount,
      supplierWEthLendingUserTabRemainingAccount,
      wethPythPriceUpdateRemainingAccount,
      supplierWBtcLendingUserTabRemainingAccount,
      wbtcPythPriceUpdateRemainingAccount,
    ]

    const refreshingRemainingAccounts = 
    [
      supplierSOLLendingUserTabRemainingAccount,
      solTokenReserveRemainingAccount,
      solSubMarketRemainingAccount,
      supplierSOLMonthlyStatementRemainingAccount,
      solPythPriceUpdateRemainingAccount,

      supplierUSDCLendingUserTabRemainingAccount,
      usdcTokenReserveRemainingAccount,
      usdcSubMarketRemainingAccount,
      supplierUSDCMonthlyStatementRemainingAccount,
      usdcPythPriceUpdateRemainingAccount,

      supplierDAILendingUserTabRemainingAccount,
      daiTokenReserveRemainingAccount,
      daiSubMarketRemainingAccount,
      supplierDAIMonthlyStatementRemainingAccount,
      daiPythPriceUpdateRemainingAccount,

      supplierWEthLendingUserTabRemainingAccount,
      wethTokenReserveRemainingAccount,
      wethSubMarketRemainingAccount,
      supplierWEthMonthlyStatementRemainingAccount,
      wethPythPriceUpdateRemainingAccount,

      supplierWBtcLendingUserTabRemainingAccount,
      wbtcTokenReserveRemainingAccount,
      wbtcSubMarketRemainingAccount,
      supplierWBtcMonthlyStatementRemainingAccount,
      wbtcPythPriceUpdateRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(successorWalletKeypair.publicKey, testUserAccountIndex)
    .signers([successorWalletKeypair])
    .remainingAccounts(refreshingRemainingAccounts)
    .instruction()

    const withdrawDAIInstruction = await program.methods.withdrawTokens(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      daiHalfDepositAmount,
      false
    )
    .accounts({ tokenMint: daiMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .remainingAccounts(remainingAccounts)
    .signers([successorWalletKeypair])
    .instruction()

    const withdrawWEthInstruction = await program.methods.withdrawTokens(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      wethHalfDepositAmount,
      false
    )
    .accounts({ tokenMint: wethMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .remainingAccounts(remainingAccounts)
    .signers([successorWalletKeypair])
    .instruction()

    const withdrawWBtcInstruction = await program.methods.withdrawTokens(
      program.provider.publicKey,
      testSubMarketIndex,
      testUserAccountIndex,
      wbtcHalfDepositAmount,
      false
    )
    .accounts({ tokenMint: wbtcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .remainingAccounts(remainingAccounts)
    .signers([successorWalletKeypair])
    .instruction()

    const { blockhash } = await program.provider.connection.getLatestBlockhash()

    const messageV0WithdrawDAI = new TransactionMessage(
    {
      payerKey: program.provider.publicKey,
      recentBlockhash: blockhash,
      instructions: [refreshUserHealthAndTokenReservesInstruction, withdrawDAIInstruction]
    }).compileToV0Message([lookUpTableAccount])

    const messageV0WithdrawWEth = new TransactionMessage(
    {
      payerKey: program.provider.publicKey,
      recentBlockhash: blockhash,
      instructions: [refreshUserHealthAndTokenReservesInstruction, withdrawWEthInstruction]
    }).compileToV0Message([lookUpTableAccount])

    const messageV0WithdrawWBtc = new TransactionMessage(
    {
      payerKey: program.provider.publicKey,
      recentBlockhash: blockhash,
      instructions: [refreshUserHealthAndTokenReservesInstruction, withdrawWBtcInstruction]
    }).compileToV0Message([lookUpTableAccount])

    //Create Versioned Transaction
    const withdrawDAITransaction = new VersionedTransaction(messageV0WithdrawDAI)
    const withdrawWEthTransaction = new VersionedTransaction(messageV0WithdrawWEth)
    const withdrawWBtcTransaction = new VersionedTransaction(messageV0WithdrawWBtc)

    const size = withdrawDAITransaction.serialize().length
    console.log(`Current Transaction Size: ${size} bytes`)

    await program.provider.sendAndConfirm(withdrawDAITransaction, [successorWalletKeypair])
    await program.provider.sendAndConfirm(withdrawWEthTransaction, [successorWalletKeypair])
    await program.provider.sendAndConfirm(withdrawWBtcTransaction, [successorWalletKeypair])
  })

  function getLendingStatsPDA()
  {
    const [lendingStatsPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("lendingStats")
      ],
      program.programId
    )
    return lendingStatsPDA
  }

  function getLendingProtocolCEOPDA()
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

  function getSolvencyTreasurerPDA()
  {
    const [solvencyTreasurerPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("solvencyTreasurer")
      ],
      program.programId
    )
    return solvencyTreasurerPDA
  }

  function getLiquidationTreasurerPDA()
  {
    const [liquidationTreasurerPDA] = anchor.web3.PublicKey.findProgramAddressSync
    (
      [
        new TextEncoder().encode("liquidationTreasurer")
      ],
      program.programId
    )
    return liquidationTreasurerPDA
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
        new anchor.BN(statementYear).toBuffer('le', 2),
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

  async function mintTokenToWallet(tokenMintAddress: PublicKey, walletATA: PublicKey)
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
        mintAmount
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
  
  async function indefiniteSleepFunction()
  {
    while(true)
    { 
      console.log('Start sleep: ', counter)
      await sleep(5000) // Sleep for 5 seconds
      console.log('End sleep: ', counter)
      counter += 1
    }
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

  async function initLookUpTable()
  {
    console.log("Creating Lookup Table")

    const slot = await program.provider.connection.getSlot();

    ([lookUpTableInstance, lookUpTableAddress] = 
    AddressLookupTableProgram.createLookupTable({
      authority: program.provider.publicKey,
      payer: program.provider.publicKey,
      recentSlot: slot,
    }))

    await timeOutFunction(1)
  }

  async function addAddressToLookUpTable(input: PublicKey | PublicKey[])
  {
    console.log("Adding Address to Lookup Table")

    const addressesToAdd = Array.isArray(input) ? input : [input]

    const extendInstruction = AddressLookupTableProgram.extendLookupTable(
    {
      payer: program.provider.wallet.publicKey,
      authority: program.provider.wallet.publicKey,
      lookupTable: lookUpTableAddress,
      addresses: addressesToAdd
    })

    await program.provider.sendAndConfirm(new Transaction().add(lookUpTableInstance, extendInstruction))

    await timeOutFunction(1)
  }
})