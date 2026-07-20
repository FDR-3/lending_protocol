import * as anchor from "@coral-xyz/anchor"
import { Program } from "@coral-xyz/anchor"
import { LendingProtocol } from "../target/types/lending_protocol"
import { assert } from "chai"
import * as fs from 'fs'
import { PublicKey,
  LAMPORTS_PER_SOL,
  Transaction,
  Keypair,
  VersionedTransaction,
  TransactionMessage,
  AddressLookupTableProgram,
  AccountMeta
} from '@solana/web3.js'
import { getLendingProtocolPDA,
  getLendingStatsPDA,
  getLendingProtocolCEOPDA,
  getSolvencyTreasurerPDA,
  getLiquidationTreasurerPDA,
  getOraclePriceValidatorPDA,
  getPriceAccountPDA,
  getTokenReservePDA,
  getSubMarketPDA,
  getLendingUserAccountPDA,
  getLendingUserTabAccountPDA,
  getlendingUserMonthlyStatementAccountPDA } from "./get_pdas"
import { Token, ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID } from "@solana/spl-token"
import { errors } from "./errors"
import { borrowWaitTimeInSeconds, useUSDCFixedBorrowAPY, baseBorrowAPY, runInsolventTest } from "./test_settings"
import type { PriceDataPayload } from "./types"
import { solTestPriceDataPayload,
  solAndUSDCTestPriceDataPayload,
  solLiquidatePriceWithUSDCDataPayload,
  usdcTestPriceDataPayload } from "./test_prices"

describe("lending_protocol", () =>
{
  //The official Token-2022 Program ID
  const TOKEN_2022_PROGRAM_ID = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")

  //Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env())

  const program = anchor.workspace.LendingProtocol as Program<LendingProtocol>

  //Just getting rid of some IDE red line errors
  if(!program.provider)
    throw new Error("Program provider is not defined")

  const programProvider = program.provider as anchor.AnchorProvider
  var programProviderPublicKey: PublicKey
  var programProviderPublicKeyString = ""
  if(programProvider.publicKey)
  {
    programProviderPublicKey = programProvider.publicKey
    programProviderPublicKeyString = programProviderPublicKey.toBase58()
  }
  
  var protocolLookUpTableAddress: PublicKey
  var protocolLookUpTableAccount: anchor.web3.AddressLookupTableAccount | null
  var mainSubMarketOwnerLookUpTableAddress: PublicKey
  var mainSubMarketOwnerLookUpTableAccount: anchor.web3.AddressLookupTableAccount | null
  var supplierLookUpTableAddress: PublicKey
  var supplierLookUpTableAccount: anchor.web3.AddressLookupTableAccount | null
  var borrowerLookUpTableAddress: PublicKey
  var borrowerLookUpTableAccount: anchor.web3.AddressLookupTableAccount | null
  var liquidatorLookUpTableAddress: PublicKey
  var liquidatorLookUpTableAccount: anchor.web3.AddressLookupTableAccount | null

  var borrowerLendingUserRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var oraclePriceValidatorRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var lendingStatsRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  const solTokenMintAddress = new PublicKey("So11111111111111111111111111111111111111112")
  const solTokenDecimalAmount = 9
  const oneSol = new anchor.BN(LAMPORTS_PER_SOL)
  const twoSol = new anchor.BN(LAMPORTS_PER_SOL * 2)
  var solTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var solTokenReserveATARemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var solSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierSOLLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerSOLLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierSOLMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerSOLMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  
  const mintAmount = 10_000_000_000

  var usdcMint: Token
  const usdcTokenDecimalAmount = 6
  const halfBorrowerUSDCAmount = new anchor.BN(35_000_000)
  const borrowerUSDCAmount = new anchor.BN(70_000_000)
  const overBorrowUSDCAmount = new anchor.BN(71_000_000)
  const lessThan10PercentOfBorrowedAmount = new anchor.BN(6_999_999) 
  const supplierUSDCAmount = new anchor.BN(100_000_000)
  var usdcTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var usdcTokenReserveATARemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var usdcSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierUSDCLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerUSDCLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierUSDCMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerUSDCMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  var daiMint: Token
  const daiTokenDecimalAmount = 8
  const daiDepositAmount = new anchor.BN(10_000_000_000)
  const daiHalfDepositAmount = new anchor.BN(5_000_000_000)
  var daiTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var daiSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierDAILendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerDAILendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierDAIMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerDAIMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  var wethMint: Token
  const wethTokenDecimalAmount = 8
  const wethDepositAmount = new anchor.BN(10_000_000_000)
  const wethHalfDepositAmount = new anchor.BN(5_000_000_000)
  var wethTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var wethSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWEthLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWEthLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWEthMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWEthMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  var wbtcMint: Token
  const wbtcTokenDecimalAmount = 8
  const wbtcDepositAmount = new anchor.BN(10_000_000_000)
  const wbtcHalfDepositAmount = new anchor.BN(5_000_000_000)
  var wbtcTokenReserveRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var wbtcSubMarketRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWBtcLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWBtcLendingUserTabRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var supplierWBtcMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }
  var borrowerWBtcMonthlyStatementRemainingAccount: { pubkey: anchor.web3.PublicKey; isSigner: boolean; isWritable: boolean }

  const borrowAPY4Percent = 400 //4.00%
  const globalLimitLow = new anchor.BN(1)
  const globalLimit1 = new anchor.BN(10_000_000_000)
  const globalLimit2 = new anchor.BN(20_000_000_000)

  const baseBorrowAPYAbove5Percent = 501 //5.01%
  const baseBorrowAPYBelove0Percent = -1 //-0.01%
  const solvencyInsuranceFeeRateAbove4Percent = 401 //4.01%
  const solvencyInsuranceFeeRateBelove0Percent = -1 //-0.01%
  const solvencyInsuranceFeeRate4Percent = 400 //4.00%
  const solvencyInsuranceFeeRate1Percent = 100 //1.00%

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
  const keypairPath = '/home/fdr-3/.config/solana/id.json'
  const keypairData = JSON.parse(fs.readFileSync(keypairPath, 'utf8'))
  const testingWalletKeypair = Keypair.fromSecretKey(Uint8Array.from(keypairData))
  const successorWalletKeypair = anchor.web3.Keypair.generate()
  const borrowerWalletKeypair = anchor.web3.Keypair.generate()
  const priceValidatorKeypair = anchor.web3.Keypair.generate()

  //Populate Oracle Address remaining account
  const oracleAddressRemainingAccount = 
  {
    pubkey: priceValidatorKeypair.publicKey,
    isSigner: false,
    isWritable: true
  }

  before(async () =>
  {
    //Fund Price Validator Wallet
    console.log("Funding Price Validator Wallet")
    await airDropSol(priceValidatorKeypair.publicKey)

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
      programProviderPublicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      usdcTokenDecimalAmount, //Decimals for USDC
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    daiMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      programProviderPublicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      daiTokenDecimalAmount, //Decimals for DAI
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    wethMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      programProviderPublicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      wethTokenDecimalAmount, //Decimals for WETH
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    wbtcMint = await Token.createMint
    (
      program.provider.connection,
      testingWalletKeypair, //Payer for the mint creation
      programProviderPublicKey, //Mint authority (who can mint tokens)
      null, //Freeze authority (opttional)
      wbtcTokenDecimalAmount, //Decimals for WBTC
      TOKEN_2022_PROGRAM_ID //SPL Token program ID
    )

    //Mint USDC to CEO Wallet
    const testingWalletUSDCATA = await deriveATA(programProviderPublicKey, usdcMint.publicKey)
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

    console.log("Setup Complete")
  })

  it("Initializes Lending Protocol", async () => 
  {
    protocolLookUpTableAddress = await initLookUpTable()

    await program.methods.initializeLendingProtocol(statementMonth, statementYear)
    .accounts({ lookUpTableAddress: protocolLookUpTableAddress })
    .rpc()

    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOPDA())
    assert(ceoAccount.address.toBase58() == programProviderPublicKeyString)

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

    //Populate Oracle Price Validator remaining account
    const oraclePriceValidatorPDA = getOraclePriceValidatorPDA()
    oraclePriceValidatorRemainingAccount = 
    {
      pubkey: oraclePriceValidatorPDA,
      isSigner: false,
      isWritable: true
    }

    //Add Lending Protocol and Lending Stats to Address Lookup Table
    await addAddressToLookUpTable(protocolLookUpTableAddress,
      [lendingProtocolPDA, lendingStatsPDA, oraclePriceValidatorPDA],
      "Lending Protocol, Lending Stats, And Oracle Price Validator")

    //Get latest lookup table
    protocolLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(protocolLookUpTableAddress)).value
  })

  it("Verifies That Only the CEO Can Pass On Account", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.passOnLendingProtocolCeo()
      .accounts({ newCeoAddress: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notCEOErrorMsg)
  })

  it("Passes on the Lending Protocol CEO Account", async () => 
  {
    await program.methods.passOnLendingProtocolCeo()
    .accounts({ newCeoAddress: successorWalletKeypair.publicKey })
    .rpc()
    
    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOPDA())
    assert(ceoAccount.address.toBase58() == successorWalletKeypair.publicKey.toBase58())
  })
  
  it("Passes back the Lending Protocol CEO Account", async () => 
  {
    await program.methods.passOnLendingProtocolCeo()
    .accounts({ newCeoAddress: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    var ceoAccount = await program.account.lendingProtocolCeo.fetch(getLendingProtocolCEOPDA())
    assert(ceoAccount.address.toBase58() == programProviderPublicKeyString)
  })

  it("Verifies That Only the Solvency Treasurer Can Pass On Account", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.passOnSolvencyTreasurer()
      .accounts({ newTreasurerAddress: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notSolvencyTreasurerErrorMsg)
  })

  it("Passes on the Solvency Treasurer Account", async () => 
  {
    await program.methods.passOnSolvencyTreasurer()
      .accounts({ newTreasurerAddress: successorWalletKeypair.publicKey })
      .rpc()

    var treasurerAccount = await program.account.solvencyTreasurer.fetch(getSolvencyTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == successorWalletKeypair.publicKey.toBase58())
  })
  
  it("Passes back the Solvency Treasurer Account", async () => 
  {
    await program.methods.passOnSolvencyTreasurer()
    .accounts({ newTreasurerAddress: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    var treasurerAccount = await program.account.solvencyTreasurer.fetch(getSolvencyTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == programProviderPublicKeyString)
  })

  it("Verifies That Only the Liquidation Treasurer Can Pass On Account", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.passOnLiquidationTreasurer()
      .accounts({ newTreasurerAddress: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notLiquidationTreasurerErrorMsg)
  })

  it("Passes on the Liquidation Treasurer Account", async () => 
  {
    await program.methods.passOnLiquidationTreasurer()
    .accounts({ newTreasurerAddress: successorWalletKeypair.publicKey })
    .rpc()

    var treasurerAccount = await program.account.liquidationTreasurer.fetch(getLiquidationTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == successorWalletKeypair.publicKey.toBase58())
  })
  
  it("Passes back the Liquidation Treasurer Account", async () => 
  {
    await program.methods.passOnLiquidationTreasurer()
    .accounts({ newTreasurerAddress: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    var treasurerAccount = await program.account.liquidationTreasurer.fetch(getLiquidationTreasurerPDA())
    assert(treasurerAccount.address.toBase58() == programProviderPublicKeyString)
  })

  it("Verifies That Only the CEO Can Set the Price Validator", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.setOraclePriceValidator()
      .accounts({ newPriceValidatorAddress: priceValidatorKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notCEOErrorMsg)
  })

  it("Sets Price Validator", async () => 
  {
    await program.methods.setOraclePriceValidator()
    .accounts({ newPriceValidatorAddress: priceValidatorKeypair.publicKey })
    .rpc()
    
    var oraclePriceValidator = await program.account.oraclePriceValidator.fetch(getOraclePriceValidatorPDA())
    assert(oraclePriceValidator.address.toBase58() == priceValidatorKeypair.publicKey.toBase58())
  })

  it("Verifies That Only the CEO Can Update the Lending Protocol Statement Month and Year", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.updateCurrentStatementMonthAndYear(newStatementMonth, newStatementYear)
      .accounts({ signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notCEOErrorMsg)
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
      await program.methods.addTokenReserve(solTokenDecimalAmount, baseBorrowAPY, true, globalLimit1, solvencyInsuranceFeeRate4Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notCEOErrorMsg)
  })

  it("Verifies That a Token Reserve Can't be Created With a Base Borrow APY Higher than 5%", async () => 
  {
    var errorMessage = ""
 
    try
    {
      await program.methods.addTokenReserve(solTokenDecimalAmount, baseBorrowAPYAbove5Percent, true, globalLimitLow, solvencyInsuranceFeeRate4Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.baseBorrowAPYTooHighErrorMsg)
  })

  it("Verifies That a Token Reserve Can't be Created With a Base Borrow APY Below 0%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.addTokenReserve(solTokenDecimalAmount, baseBorrowAPYBelove0Percent, true, globalLimitLow, solvencyInsuranceFeeRate4Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.code
    }

    assert(errorMessage == errors.outOfRangeError)
  })

  it("Verifies That a Token Reserve Can't be Created With a Solvency Insurance Fee on Interest Rate Higher than 4%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.addTokenReserve(solTokenDecimalAmount, baseBorrowAPY, true, globalLimitLow, solvencyInsuranceFeeRateAbove4Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.solvencyInsuranceFeeOnInterestEarnedRateTooHighErrorMsg)
  })

  it("Verifies That a Token Reserve Can't be Created With a Solvency Insurance Fee on Interest Rate Below 0%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.addTokenReserve(solTokenDecimalAmount, baseBorrowAPY, true, globalLimitLow, solvencyInsuranceFeeRateBelove0Percent)
      .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.code
    }

    assert(errorMessage == errors.outOfRangeError)
  })
  
  it("Adds a wSOL Token Reserve", async () => 
  {
    await program.methods.addTokenReserve(solTokenDecimalAmount, baseBorrowAPY, true, globalLimitLow, solvencyInsuranceFeeRate4Percent)
    .accounts({ tokenMint: solTokenMintAddress, tokenProgram: TOKEN_PROGRAM_ID })
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenId == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.borrowApy == baseBorrowAPY)
    assert(tokenReserve.globalLimit.eq(globalLimitLow))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate4Percent)

    //Populate SOL Token Reserve remaining account
    const solTokenReservePDA = getTokenReservePDA(solTokenMintAddress)
    solTokenReserveRemainingAccount = 
    {
      pubkey: solTokenReservePDA,
      isSigner: false,
      isWritable: true
    }
    const tokenReserveSolATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    solTokenReserveATARemainingAccount = 
    {
      pubkey: tokenReserveSolATA,
      isSigner: false,
      isWritable: true
    }

    //Add Token Reserve to Address Lookup Table
    await addAddressToLookUpTable(protocolLookUpTableAddress, solTokenReservePDA, "SOL Token Reserve")

    //Get latest lookup table
    protocolLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(protocolLookUpTableAddress)).value
  })

  it("Verifies That a SubMarket Can't be Created With a Fee on Interest Rate Higher than 100%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRateAbove100Percent, null)
      .accounts({ tokenMintAddress: solTokenMintAddress, feeCollectorAddress: programProviderPublicKey })
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.subMarketFeeOnInterestEarnedRateTooHighErrorMsg)
  })

  it("Verifies That a SubMarket Can't be Created With a Fee on Interest Rate Below 0%", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRateBelove0Percent, null)
      .accounts({ tokenMintAddress: solTokenMintAddress, feeCollectorAddress: programProviderPublicKey })
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.code
    }

    assert(errorMessage == errors.outOfRangeError)
  })

  it("Verifies That a Look Up Table Address is Required when a Sub Market Owner Creates Their First Sub Market", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRate8Percent, null)
      .accounts({ tokenMintAddress: solTokenMintAddress, feeCollectorAddress: programProviderPublicKey })
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.subMarketOwnerLookUpTableMissingErrorMsg)
  })

  it("Creates a wSOL SubMarket", async () => 
  {
    mainSubMarketOwnerLookUpTableAddress = await initLookUpTable()
    
    await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRate8Percent, mainSubMarketOwnerLookUpTableAddress)
    .accounts({ tokenMintAddress: solTokenMintAddress, feeCollectorAddress: programProviderPublicKey })
    .rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == programProviderPublicKeyString)
    assert(subMarket.feeCollectorAddress.toBase58() == programProviderPublicKeyString)
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(subMarket.tokenId == solTestPriceDataPayload.data[0].tokenId)
    assert(subMarket.subMarketIndex == testSubMarketIndex)

    //Populate SOL SubMarket Remaining Account
    const solSubMarketPDA = getSubMarketPDA(solTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex)
    solSubMarketRemainingAccount = 
    {
      pubkey: solSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Add SubMarket to Address Lookup Table
    await addAddressToLookUpTable(mainSubMarketOwnerLookUpTableAddress, solSubMarketPDA, "SOL SubMarket")

    //Get latest lookup table
    mainSubMarketOwnerLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(mainSubMarketOwnerLookUpTableAddress)).value
  })

  it("Edits a wSOL SubMarket", async () => 
  {
    await program.methods.editSubMarket(solTestPriceDataPayload.data[0].tokenId, testSubMarketIndex, subMarketFeeRate100Percent)
    .accounts({ feeCollectorAddress: successorWalletKeypair.publicKey })
    .rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    
    assert(subMarket.owner.toBase58() == programProviderPublicKeyString)
    assert(subMarket.feeCollectorAddress.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate100Percent)
    assert(subMarket.tokenId == solTestPriceDataPayload.data[0].tokenId)
    assert(subMarket.subMarketIndex == testSubMarketIndex)
  })

  //Because the SubMarket account is derived from the signer calling the function (and not passed into the function based on trust), it's never possible to even try to edit someone else's Sub Market
  it("Verifies That a SubMarket Can Only be Edited by the Owner", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.editSubMarket(solTestPriceDataPayload.data[0].tokenId, testSubMarketIndex, subMarketFeeRate100Percent)
      .accounts({ feeCollectorAddress: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.expectedThisAccountToExistErrorMsg)
  })

  it("Verifies you can't Deposit Over the Global Limit", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, twoSol, accountName, null)
      .accounts({
        tokenMint: solTokenMintAddress,
        subMarketOwner: programProviderPublicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.globalLimitExceededErrorMsg)
  })

  it("Verifies That Only the CEO Can Update the Token Reserve", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.updateTokenReserve(borrowAPY4Percent, true, globalLimit1, solvencyInsuranceFeeRate4Percent)
      .accounts({ tokenMintAddress: solTokenMintAddress, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notCEOErrorMsg)
  })

  it("Updates Token Reserve Borrow APY, Global Limit, and Solvency Insurance Rate", async () => 
  {
    await program.methods.updateTokenReserve(borrowAPY4Percent, true, globalLimit2, solvencyInsuranceFeeRate1Percent)
    .accounts({ tokenMintAddress: solTokenMintAddress })
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.borrowApy == borrowAPY4Percent)
    assert(tokenReserve.globalLimit.eq(globalLimit2))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate1Percent)
  })

  it("Deposits wSOL Into the Token Reserve", async () => 
  {
    supplierLookUpTableAddress = await initLookUpTable()

    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, twoSol, accountName, supplierLookUpTableAddress)
    .accounts({
        tokenMint: solTokenMintAddress,
        subMarketOwner: programProviderPublicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenId == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(twoSol))

    const tokenReserveATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    const tokenReserveATAAccount = await program.provider.connection.getTokenAccountBalance(tokenReserveATA)
    assert(parseInt(tokenReserveATAAccount.value.amount) == twoSol.toNumber())

    const supplierLendingUserAccountPDA = getLendingUserAccountPDA
    (
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    const lendingUserAccount = await program.account.lendingUserAccount.fetch(supplierLendingUserAccountPDA)
    assert(lendingUserAccount.accountName == accountName)
    assert(lendingUserAccount.tabAccountCount == 1)

    const successorSOLLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(successorSOLLendingUserTabAccountPDA)
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenId == solTestPriceDataPayload.data[0].tokenId)
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == programProviderPublicKeyString)
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 0)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(twoSol))

    const supplierSOLMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    )
    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(supplierSOLMonthlyStatementPDA)
    assert(lendingUserMonthlyStatementAccount.statementMonth == newStatementMonth)
    assert(lendingUserMonthlyStatementAccount.statementYear == newStatementYear)
    assert(lendingUserMonthlyStatementAccount.snapShotBalanceAmount.eq(twoSol))
    assert(lendingUserMonthlyStatementAccount.monthlyDepositedAmount.eq(twoSol))

    //Populate Supplier SOL Tab Remaining Account
    supplierSOLLendingUserTabRemainingAccount = 
    {
      pubkey: successorSOLLendingUserTabAccountPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Supplier SOL Monthly Statement Remaining Account
    supplierSOLMonthlyStatementRemainingAccount = 
    {
      pubkey: supplierSOLMonthlyStatementPDA,
      isSigner: false,
      isWritable: true
    }

    //Add Lending User Tab and Monthly Statment Accounts to Address Lookup Table
    await addAddressToLookUpTable
    (
      supplierLookUpTableAddress,
      [supplierLendingUserAccountPDA, successorSOLLendingUserTabAccountPDA, supplierSOLMonthlyStatementPDA],
      "Lending User Tab and Monthly Statement"
    )

    //Get latest lookup table
    supplierLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(supplierLookUpTableAddress)).value
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
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.accountNameTooLongErrorMsg)
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
      const withdrawInstruction = await program.methods.withdrawTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        tooMuchSol,
        false)
      .accounts({
        tokenMint: solTokenMintAddress,
        subMarketOwner: programProviderPublicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .instruction()

      await sendVersionedTrasaction([withdrawInstruction], [successorWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.insufficientFundsErrorMsg))
  })

  it("Withdraws wSOL From the Token Reserve", async () => 
  {
    const withdrawInstruction = await program.methods.withdrawTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      twoSol,
      true)
    .accounts({
      tokenMint: solTokenMintAddress,
      subMarketOwner: programProviderPublicKey,
      tokenProgram: TOKEN_PROGRAM_ID,
      signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .instruction()

    await sendVersionedTrasaction([withdrawInstruction], [successorWalletKeypair])

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.tokenId == 1)
    assert(tokenReserve.tokenMintAddress.toBase58() == solTokenMintAddress.toBase58())
    assert(tokenReserve.tokenDecimalAmount == solTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))

    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenId == solTestPriceDataPayload.data[0].tokenId)
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == programProviderPublicKeyString)
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
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
    catch(error: any)
    {
      errorMessage = error.message
    }

    assert(errorMessage == errors.ataDoesNotExistErrorMsg + userATA + " not found")
  })
  
  it("Adds a USDC Token Reserve", async () => 
  {
    await program.methods.addTokenReserve(usdcTokenDecimalAmount, baseBorrowAPY, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate4Percent)
    .accounts({ tokenMint: usdcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()
    
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenId == 2)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    assert(tokenReserve.borrowApy == baseBorrowAPY)
    assert(tokenReserve.globalLimit.eq(globalLimit1))
    assert(tokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate4Percent)

    //Populate USDC Token Reserve remaining account
    const usdcTokenReservePDA = getTokenReservePDA(usdcMint.publicKey)
    usdcTokenReserveRemainingAccount = 
    {
      pubkey: usdcTokenReservePDA,
      isSigner: false,
      isWritable: true
    }
    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    usdcTokenReserveATARemainingAccount = 
    {
      pubkey: tokenReserveUSDCATA,
      isSigner: false,
      isWritable: true
    }

    //Add Token Reserve to Address Lookup Table
    await addAddressToLookUpTable(protocolLookUpTableAddress, usdcTokenReservePDA, "USDC Token Reserve")

    //Get latest lookup table
    protocolLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(protocolLookUpTableAddress)).value
  })

  it("Creates a USDC SubMarket", async () => 
  {
    await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRate8Percent, null)
    .accounts({ tokenMintAddress: usdcMint.publicKey, feeCollectorAddress: programProviderPublicKey })
    .rpc()

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    assert(subMarket.owner.toBase58() == programProviderPublicKeyString)
    assert(subMarket.feeCollectorAddress.toBase58() == programProviderPublicKeyString)
    assert(subMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(subMarket.tokenId == usdcTestPriceDataPayload.data[0].tokenId)
    assert(subMarket.subMarketIndex == testSubMarketIndex)

    //Populate USDC SubMarket Remaining Account
    const usdcSubMarketPDA = getSubMarketPDA(usdcTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex)
    usdcSubMarketRemainingAccount = 
    {
      pubkey: usdcSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Add SubMarket to Address Lookup Table
    await addAddressToLookUpTable(mainSubMarketOwnerLookUpTableAddress, usdcSubMarketPDA, "USDC SubMarket")

    //Get latest lookup table
    mainSubMarketOwnerLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(mainSubMarketOwnerLookUpTableAddress)).value
  })

  it("Deposits USDC Into the Token Reserve", async () => 
  {
    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, supplierUSDCAmount, null, null)
    .accounts({
      tokenMint: usdcMint.publicKey,
      subMarketOwner: programProviderPublicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
   
    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.tokenId == 2)
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
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenId == usdcTestPriceDataPayload.data[0].tokenId)
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == programProviderPublicKeyString)
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 1)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(supplierUSDCAmount))

    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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

    //Add Lending User Tab and Monthly Statment Accounts to Address Lookup Table
    await addAddressToLookUpTable(supplierLookUpTableAddress, [usdcLendingUserTabAccountPDA, supplierUSDCMonthlyStatementPDA], "Lending User Tab and Monthly Statement")

    //Get latest lookup table
    supplierLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(supplierLookUpTableAddress)).value
  })

  it("Deposits 1 SOL as Collateral", async () => 
  {
    borrowerLookUpTableAddress = await initLookUpTable()

    //Depositing 1 Sol as Collateral
    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, oneSol, accountName, borrowerLookUpTableAddress)
    .accounts({
      tokenMint: solTokenMintAddress,
      subMarketOwner: programProviderPublicKey,
      tokenProgram: TOKEN_PROGRAM_ID,
      signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .rpc()

    const borrowerLendingUserAccountPDA = getLendingUserAccountPDA
    (
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    )
    borrowerLendingUserRemainingAccount = 
    {
      pubkey: borrowerLendingUserAccountPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate Borrower SOL Tab Remaining Account
    const borrowerSOLLendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
 
    //Add Lending User Tab and Monthly Statment Accounts to Address Lookup Table
    await addAddressToLookUpTable
    (
      borrowerLookUpTableAddress,
      [
        borrowerLendingUserAccountPDA,
        borrowerSOLLendingUserTabAccountPDA,
        borrowerUSDCLendingUserTabAccountPDA,
        borrowerSOLMonthlyStatementPDA,
        borrowerUSDCMonthlyStatementPDA
      ],
      "Lending User Tab and Monthly Statement"
      )

    //Get latest lookup table
    borrowerLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(borrowerLookUpTableAddress)).value
  })

  it("Verifies only the Oracle Price Validator can set prices", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.createTempOraclePriceData(solTestPriceDataPayload)
      .accounts({ lendingUserAddress: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notPriceOracle)
  })

  it("Verifies you Can't Refresh User's Health Without Their Tab Account", async () => 
  {
    var errorMessage = ""

    try
    {
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solTestPriceDataPayload, successorWalletKeypair.publicKey)

      //Refresh Token Reserve and User Health
      const remainingAccounts =
      [
        priceRemainingAccount,
        solTokenReserveRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount
      ]

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
        
      await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 1, 1, true)
      .accounts({lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.unexpectedTabAccountErrorMsg)
  })

  it("Verifies you Can't Refresh User's Health Without the Right Token Reserve", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(successorWalletKeypair.publicKey, successorWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solTestPriceDataPayload, successorWalletKeypair.publicKey)
      
      //Refresh Token Reserve and User Health
      const remainingAccounts =
      [
        priceRemainingAccount,
        usdcTokenReserveRemainingAccount,
        supplierSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount
      ]

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })

      await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 1, 1, true)
      .accounts({lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.missingExpectedTokenReserveErrorMsg)
  })

  it("Verifies you Can't Refresh User's Health Without the Right SubMarket", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(successorWalletKeypair.publicKey, successorWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solTestPriceDataPayload, successorWalletKeypair.publicKey)

      //Refresh Token Reserve and User Health
      const remainingAccounts =
      [
        priceRemainingAccount,
        solTokenReserveRemainingAccount,
        supplierSOLLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount
      ]

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })

      await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 1, 1, true)
      .accounts({lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.unexpectedSubMarketErrorMsg)
  })

  it("Verifies you Can't Refresh User's Health Without the Right Monthly Statement", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(successorWalletKeypair.publicKey, successorWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solTestPriceDataPayload, successorWalletKeypair.publicKey)

      //Refresh Token Reserve and User Health
      const remainingAccounts =
      [
        priceRemainingAccount,
        solTokenReserveRemainingAccount,
        supplierSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount
      ]

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })

      await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 1, 1, true)
      .accounts({lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(remainingAccounts)
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.unexpectedMonthlyStatementErrorMsg)
  })

  it("Verifies a User Can't Borrow When the Lending User's Health Data is Stale", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.borrowTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        overBorrowUSDCAmount,
        false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.staleTokenReserveOrLendingUserErrorMsg)
  })

  it("Verifies that you can't Borrow More than 70% of the Value of your Collateral", async () => 
  {
    var errorMessage = ""

    try
    {
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, borrowerWalletKeypair.publicKey)

      const refreshingRemainingAccounts =
      [
        priceRemainingAccount,
        solTokenReserveRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 1, 1, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([borrowerWalletKeypair])
      .instruction()

      const borrowInstruction = await program.methods.borrowTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        overBorrowUSDCAmount,
        false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount])
      .signers([borrowerWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })

      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, borrowInstruction], [borrowerWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.debtExceeding70PercentOfCollateralErrorMsg))
  })

  it("Verifies that you can't Borrow from a new reserve (new to the user) without including it's price data", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(borrowerWalletKeypair.publicKey, borrowerWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solTestPriceDataPayload, borrowerWalletKeypair.publicKey)

      //Borrowing from the USDC that the Successor deposited
      const refreshingRemainingAccounts =
      [
        priceRemainingAccount,
        solTokenReserveRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 1, 1, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([borrowerWalletKeypair])
      .instruction()

      const borrowInstruction = await program.methods.borrowTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount])
      .signers([borrowerWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })

      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, borrowInstruction], [borrowerWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.oraclePriceNotFoundErrorMsg))
  })

  it("Borrows USDC From the Token Reserve", async () => 
  {
    await closeUserPreviousTempOraclePriceDataAccount(borrowerWalletKeypair.publicKey, borrowerWalletKeypair)
    const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, borrowerWalletKeypair.publicKey)

    //Borrowing from the USDC that the Successor deposited
    const refreshingRemainingAccounts =
    [
      priceRemainingAccount,
      solTokenReserveRemainingAccount,
      borrowerSOLLendingUserTabRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 1, 1, false)
    .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts(refreshingRemainingAccounts)
    .signers([borrowerWalletKeypair])
    .instruction()

    const borrowInstruction = await program.methods.borrowTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      borrowerUSDCAmount,
      false)
    .accounts({
      subMarketOwner: programProviderPublicKey,
      tokenMint: usdcMint.publicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
    .signers([borrowerWalletKeypair])
    .instruction()

    await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
    await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, borrowInstruction], [borrowerWalletKeypair])

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    console.log("Token Reserve Supply Interest Change Index: ", Number(tokenReserve.supplyInterestChangeIndex))
    console.log("Token Reserve Borrow Interest Change Index: ", Number(tokenReserve.borrowInterestChangeIndex))
    assert(tokenReserve.borrowedAmount.eq(borrowerUSDCAmount))
    assert(tokenReserve.supplyApy == tokenReserve.borrowApy * tokenReserve.utilizationRate / 10000)
    assert(tokenReserve.utilizationRate == Number(tokenReserve.borrowedAmount) / Number(tokenReserve.depositedAmount) * 10000)
    
    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, borrowerWalletKeypair.publicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([borrowerWalletKeypair])
      .instruction()

      const withdrawInstruction = await program.methods.withdrawTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        new anchor.BN(1),
        false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: solTokenMintAddress,
        tokenProgram: TOKEN_PROGRAM_ID,
        signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
      .signers([borrowerWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, withdrawInstruction], [borrowerWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }
    
    assert(errorMessage.includes(errors.debtExceeding70PercentOfCollateralErrorMsg))

    //Allow some time after borrow for interest to increase
    //This was placed here and not the previous borrow test to allow this test to pass. Can't have interest already being earned, increasing the withdrawable amount.
    await timeOutFunction(borrowWaitTimeInSeconds)
  })

  it("Verifies you can't Withdraw When too many Tokens are Currently Being Borrowed", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(successorWalletKeypair.publicKey, successorWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, successorWalletKeypair.publicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        supplierSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,

        supplierUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([successorWalletKeypair])
      .instruction()

      const withdrawInstruction = await program.methods.withdrawTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: successorWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
      .signers([successorWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, withdrawInstruction], [successorWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.insufficientLiquidityErrorMsg))
  })

  it("Verifies you can't Borrow When too many Tokens are Currently Being Borrowed", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(successorWalletKeypair.publicKey, successorWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, successorWalletKeypair.publicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        supplierSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount,

        supplierUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .signers([successorWalletKeypair])
      .instruction()

      const borrowInstruction = await program.methods.borrowTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: successorWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
      .signers([successorWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, borrowInstruction], [successorWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.insufficientLiquidityErrorMsg))
  })

  it("Verifies you can't liquidate an Account whose Debt Value is less than 80% of its Collateral Value", async () => 
  {
    var errorMessage = ""

    try
    {
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, programProviderPublicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      const liquidationRemainingAccounts =
      [
        borrowerLendingUserRemainingAccount,
        usdcTokenReserveATARemainingAccount,
        solTokenReserveATARemainingAccount,
        oraclePriceValidatorRemainingAccount,
        priceRemainingAccount,
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        oracleAddressRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        testSubMarketIndex,
        testSubMarketIndex,
        testUserAccountIndex,
        testUserAccountIndex,
        halfBorrowerUSDCAmount,
        true,
        false,
        false,
        null,
        null)
      .accounts({
        liquidatiAccountOwner: borrowerWalletKeypair.publicKey,
        repaymentSubMarketOwner: programProviderPublicKey,
        liquidationSubMarketOwner: programProviderPublicKey,
        repaymentMint: usdcMint.publicKey,
        liquidationMint: solTokenMintAddress,
        repaymentTokenProgram: TOKEN_2022_PROGRAM_ID,
        liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction], [])
      await sendVersionedTrasaction([liquidateInstruction], [])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.notLiquidatableErrorMsg))
  })

  it("Verifies a liquidator can't zero out an Account whose Debt Value is less than 100% of its Collateral Value", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(programProviderPublicKey, testingWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, programProviderPublicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      const liquidationRemainingAccounts =
      [
        borrowerLendingUserRemainingAccount,
        usdcTokenReserveATARemainingAccount,
        solTokenReserveATARemainingAccount,
        oraclePriceValidatorRemainingAccount,
        priceRemainingAccount,
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        oracleAddressRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        testSubMarketIndex,
        testSubMarketIndex,
        testUserAccountIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        false,
        true,
        false,
        null,
        null)
      .accounts({
        liquidatiAccountOwner: borrowerWalletKeypair.publicKey,
        repaymentSubMarketOwner: programProviderPublicKey,
        liquidationSubMarketOwner: programProviderPublicKey,
        repaymentMint: usdcMint.publicKey,
        liquidationMint: solTokenMintAddress,
        repaymentTokenProgram: TOKEN_2022_PROGRAM_ID,
        liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction], [])
      await sendVersionedTrasaction([liquidateInstruction], [])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.notInsolventErrorMsg))
  })

  it("Verifies a liquidator can't repay more than 50% of someone's debt when liquidating an account that is in a bad state but not completely insolvent", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(programProviderPublicKey, testingWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, programProviderPublicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      const liquidationRemainingAccounts =
      [
        borrowerLendingUserRemainingAccount,
        usdcTokenReserveATARemainingAccount,
        solTokenReserveATARemainingAccount,
        oraclePriceValidatorRemainingAccount,
        priceRemainingAccount,
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        oracleAddressRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        testSubMarketIndex,
        testSubMarketIndex,
        testUserAccountIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        false,
        false,
        false,
        null,
        null)
      .accounts({
        liquidatiAccountOwner: borrowerWalletKeypair.publicKey,
        repaymentSubMarketOwner: programProviderPublicKey,
        liquidationSubMarketOwner: programProviderPublicKey,
        repaymentMint: usdcMint.publicKey,
        liquidationMint: solTokenMintAddress,
        repaymentTokenProgram: TOKEN_2022_PROGRAM_ID,
        liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction], [])
      await sendVersionedTrasaction([liquidateInstruction], [])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.overLiquidationErrorMsg))
  })

  it("Verifies a User Can't Repay When the Lending User's Health Data is Stale", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(programProviderPublicKey, testingWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solAndUSDCTestPriceDataPayload, programProviderPublicKey)

      const repayTokenInstruction = await program.methods.repayTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      lessThan10PercentOfBorrowedAmount,
      false,
      false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
      .signers([borrowerWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([repayTokenInstruction], [borrowerWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.staleTokenReserveOrLendingUserErrorMsg))
  })

  it("Verifies a Borrower can't Repay less than 10% when their account is in a bad state to prevent 'griefing'.", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(borrowerWalletKeypair.publicKey, borrowerWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, borrowerWalletKeypair.publicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      const repayTokenInstruction = await program.methods.repayTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      lessThan10PercentOfBorrowedAmount,
      false,
      false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
      .signers([borrowerWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, repayTokenInstruction], [borrowerWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.griefingErrorMsg))
  })

  it("Verifies a liquidator can't repay less than 10% of an account in a bad state to prevent 'griefing'", async () => 
  {
    var errorMessage = ""

    try
    {
      await closeUserPreviousTempOraclePriceDataAccount(programProviderPublicKey, testingWalletKeypair)
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, programProviderPublicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey })
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      const liquidationRemainingAccounts =
      [
        borrowerLendingUserRemainingAccount,
        usdcTokenReserveATARemainingAccount,
        solTokenReserveATARemainingAccount,
        oraclePriceValidatorRemainingAccount,
        priceRemainingAccount,
        lendingStatsRemainingAccount,
        usdcSubMarketRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerUSDCLendingUserTabRemainingAccount,
        borrowerSOLLendingUserTabRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,
        oracleAddressRemainingAccount
      ]

      const liquidateInstruction = await program.methods.liquidateAccount(
        testSubMarketIndex,
        testSubMarketIndex,
        testUserAccountIndex,
        testUserAccountIndex,
        lessThan10PercentOfBorrowedAmount,
        false,
        false,
        false,
        null,
        null)
      .accounts({
        liquidatiAccountOwner: borrowerWalletKeypair.publicKey,
        repaymentSubMarketOwner: programProviderPublicKey,
        liquidationSubMarketOwner: programProviderPublicKey,
        repaymentMint: usdcMint.publicKey,
        liquidationMint: solTokenMintAddress,
        repaymentTokenProgram: TOKEN_2022_PROGRAM_ID,
        liquidationTokenProgram: TOKEN_PROGRAM_ID })
      .remainingAccounts(liquidationRemainingAccounts)
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction], [])
      await sendVersionedTrasaction([liquidateInstruction], [])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.griefingErrorMsg))
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

    var liquidationSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    var liquidationTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    var liquidationTokenReserveUSDCATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    var liquidationTokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(liquidationTokenReserveUSDCATA)
    console.log("Liquidation Token Reserve Deposited Amount Before Liquidation", Number(liquidationTokenReserve.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Liquidated Amount Before Liquidation", Number(liquidationTokenReserve.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Sub Market Liquidation Fees Generated Amount Before Liquidation", Number(liquidationSubMarket.liquidationFeesGeneratedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Uncollected Liquidation Fee Amount Before Liquidation", Number(liquidationTokenReserve.uncollectedLiquidationFeesAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Wallet Balance Before Liquidation", liquidationTokenReserveUSDCATABalance.value.uiAmount, "SOL", "\n")

    var liquidatiRepaymentLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Borrowed Amount Before Liquidation", Number(liquidatiRepaymentLendingUserTabAccount.borrowedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")

    var liquidatiLiquidationLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Deposited Amount Before Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidati Liquidated Amount Before Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")

    await closeUserPreviousTempOraclePriceDataAccount(programProviderPublicKey, testingWalletKeypair)
    const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, programProviderPublicKey)
    
    const refreshingRemainingAccounts =
    [
      //Price Account
      priceRemainingAccount,

      //Token Reserves
      solTokenReserveRemainingAccount,
      usdcTokenReserveRemainingAccount,

      //Sets of Tabs, Submarkets, and Monthly Statement Accounts
      borrowerSOLLendingUserTabRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,

      borrowerUSDCLendingUserTabRemainingAccount,
      usdcSubMarketRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
    .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey })
    .remainingAccounts(refreshingRemainingAccounts)
    .instruction()

    const liquidationRemainingAccounts =
    [
      borrowerLendingUserRemainingAccount,
      usdcTokenReserveATARemainingAccount,
      solTokenReserveATARemainingAccount,
      oraclePriceValidatorRemainingAccount,
      priceRemainingAccount,
      lendingStatsRemainingAccount,
      usdcSubMarketRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerUSDCLendingUserTabRemainingAccount,
      borrowerSOLLendingUserTabRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,
      oracleAddressRemainingAccount
    ]

    liquidatorLookUpTableAddress = await initLookUpTable()
    const liquidateInstruction = await program.methods.liquidateAccount(
      testSubMarketIndex,
      testSubMarketIndex,
      testUserAccountIndex,
      testUserAccountIndex,
      halfBorrowerUSDCAmount,
      true,
      runInsolventTest,
      false,
      null,
      liquidatorLookUpTableAddress)
    .accounts({
      liquidatiAccountOwner: borrowerWalletKeypair.publicKey,
      repaymentSubMarketOwner: programProviderPublicKey,
      liquidationSubMarketOwner: programProviderPublicKey,
      repaymentMint: usdcMint.publicKey,
      liquidationMint: solTokenMintAddress,
      repaymentTokenProgram: TOKEN_2022_PROGRAM_ID,
      liquidationTokenProgram: TOKEN_PROGRAM_ID })
    .remainingAccounts(liquidationRemainingAccounts)
    .instruction()

    await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
    await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction,], [])
    await sendVersionedTrasactionWithHigherCompute([liquidateInstruction], [])

    console.log("<-- After Liquidation -->")

    var repaymentTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    var repaymentTokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    var repaymentTokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(repaymentTokenReserveUSDCATA)
    console.log("Repayment Token Reserve Deposited Amount After Liquidation", Number(repaymentTokenReserve.depositedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Borrowed Amount After Liquidation", Number(repaymentTokenReserve.borrowedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Repaid Debt After Liquidation", Number(repaymentTokenReserve.repaidDebtAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    console.log("Repayment Token Reserve Wallet Balance After Liquidation", repaymentTokenReserveUSDCATABalance.value.uiAmount, "USDC", "\n")

    var liquidationSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    var liquidationTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    var liquidationTokenReserveSOLATA = await deriveATA(getTokenReservePDA(solTokenMintAddress), solTokenMintAddress, true)
    var liquidationTokenReserveSOLATABalance = await program.provider.connection.getTokenAccountBalance(liquidationTokenReserveSOLATA)
    console.log("Liquidation Token Reserve Deposited Amount After Liquidation", Number(liquidationTokenReserve.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Liquidated Amount After Liquidation", Number(liquidationTokenReserve.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Sub Market Liquidation Fees Generated Amount After Liquidation", Number(liquidationSubMarket.liquidationFeesGeneratedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Uncollected Liquidation Fee Amount After Liquidation", Number(liquidationTokenReserve.uncollectedLiquidationFeesAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidation Token Reserve Wallet Balance After Liquidation", liquidationTokenReserveSOLATABalance.value.uiAmount, "SOL", "\n")

    const liquidatorLendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      programProviderPublicKey,
      testUserAccountIndex
    ))
    assert(liquidatorLendingUserAccount.accountName == "Generic Liquidator")
    assert(liquidatorLendingUserAccount.tabAccountCount == 2)

    const liquidatorLiquidationLendingUserTabPDA = getLendingUserTabAccountPDA
    (
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      programProviderPublicKey,
      testUserAccountIndex
    )
    const liquidatorLiquidationLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(liquidatorLiquidationLendingUserTabPDA)
    console.log("Liquidator Liquidation Amount After Liquidation", Number(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidator Fees Generated Amount After Liquidation", Number(liquidatorLiquidationLendingUserTabAccount.feesGeneratedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")
    assert(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount.gt(bnZero))
    assert(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount.eq(liquidatorLiquidationLendingUserTabAccount.depositedAmount))

    var liquidatiRepaymentLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Borrowed Amount After Liquidation", Number(liquidatiRepaymentLendingUserTabAccount.borrowedAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    assert(liquidatiRepaymentLendingUserTabAccount.borrowedAmount.eq(repaymentTokenReserve.borrowedAmount))

    var liquidatiLiquidationLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Deposited Amount After Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.depositedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidati Liquidated Amount After Liquidation", Number(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    assert(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount.eq(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount.add(liquidatorLiquidationLendingUserTabAccount.feesGeneratedAmount)))
    assert(oneSol.eq(liquidatiLiquidationLendingUserTabAccount.depositedAmount.add(liquidatiLiquidationLendingUserTabAccount.liquidatedAmount)))

    const liquidatiRepaymentMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati SnapShot Debt Balance Amount After Liquidation", Number(liquidatiRepaymentMonthlyStatementAccount.snapShotDebtAmount) / Math.pow(10, repaymentTokenReserve.tokenDecimalAmount), "USDC")
    assert(liquidatiRepaymentMonthlyStatementAccount.snapShotDebtAmount.eq(liquidatiRepaymentLendingUserTabAccount.borrowedAmount))
    
    const liquidatiLiquidationMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    console.log("Liquidati Monthly Statement Liquidated Amount After Liquidation", Number(liquidatiLiquidationMonthlyStatementAccount.monthlyLiquidatedAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidati SnapShot Deposit Balance Amount After Liquidation", Number(liquidatiLiquidationMonthlyStatementAccount.snapShotBalanceAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")
    assert(liquidatiLiquidationMonthlyStatementAccount.snapShotBalanceAmount.eq(oneSol.sub(liquidatiLiquidationMonthlyStatementAccount.monthlyLiquidatedAmount)))
 
    const liquidatorLiquidationMonthlyStatementPDA = getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      programProviderPublicKey,
      testUserAccountIndex
    )
    const liquidatorLiquidationMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(liquidatorLiquidationMonthlyStatementPDA)
    console.log("Liquidator Monthly Statement Liquidated Amount After Liquidation", Number(liquidatorLiquidationMonthlyStatementAccount.monthlyLiquidatorAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL")
    console.log("Liquidator SnapShot Deposit Balance Amount After Liquidation", Number(liquidatorLiquidationMonthlyStatementAccount.snapShotBalanceAmount) / Math.pow(10, liquidationTokenReserve.tokenDecimalAmount), "SOL", "\n")
    assert(liquidatorLiquidationMonthlyStatementAccount.monthlyLiquidatorAmount.eq(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount))
    assert(liquidatorLiquidationMonthlyStatementAccount.snapShotBalanceAmount.eq(liquidatorLiquidationLendingUserTabAccount.liquidatorAmount))

    //Add Lending User Tab and Monthly Statment Accounts to Address Lookup Table
    await addAddressToLookUpTable
    (
      liquidatorLookUpTableAddress,
      [liquidatorLiquidationLendingUserTabPDA, liquidatorLiquidationMonthlyStatementPDA],
      "Liquidator Lending User Tab and Monthly Statement"
    )

    //Get latest lookup table
    liquidatorLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(liquidatorLookUpTableAddress)).value
  })
 
  it("Refreshes Token Reserves and Supplier's/Borrower's Health Status", async () => 
  {
    await closeUserPreviousTempOraclePriceDataAccount(successorWalletKeypair.publicKey, successorWalletKeypair)
    const [supplierUpdatePricesTransaction, supplierPriceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, successorWalletKeypair.publicKey)
    
    //Refresh Supplier
    const refreshingSupplierRemainingAccounts =
    [
      //Price Account
      supplierPriceRemainingAccount,
      
      //Token Reserves
      solTokenReserveRemainingAccount,
      usdcTokenReserveRemainingAccount,

      //Sets of Tabs, Submarkets, and Monthly Statement Accounts
      supplierSOLLendingUserTabRemainingAccount,
      solSubMarketRemainingAccount,
      supplierSOLMonthlyStatementRemainingAccount,

      supplierUSDCLendingUserTabRemainingAccount,
      usdcSubMarketRemainingAccount,
      supplierUSDCMonthlyStatementRemainingAccount,

      //Oracle Account
      oracleAddressRemainingAccount
    ]

    await program.provider.connection.sendRawTransaction(supplierUpdatePricesTransaction.serialize(), { skipPreflight: false })

    await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, true)
    .accounts({ lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
    .remainingAccounts(refreshingSupplierRemainingAccounts)
    .signers([successorWalletKeypair])
    .rpc()

    //Refresh Borrower
    await closeUserPreviousTempOraclePriceDataAccount(borrowerWalletKeypair.publicKey, borrowerWalletKeypair)
    const [borrowerUpdatePricesTransaction, borrowerPriceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, borrowerWalletKeypair.publicKey)

    const refreshingBorrowerRemainingAccounts =
    [
      //Price Account
      borrowerPriceRemainingAccount,

      //Token Reserves
      solTokenReserveRemainingAccount,
      usdcTokenReserveRemainingAccount,

      //Sets of Tabs, Submarkets, and Monthly Statement Accounts
      borrowerSOLLendingUserTabRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,

      borrowerUSDCLendingUserTabRemainingAccount,
      usdcSubMarketRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount,

      //Oracle Account
      oracleAddressRemainingAccount
    ]

    await program.provider.connection.sendRawTransaction(borrowerUpdatePricesTransaction.serialize(), { skipPreflight: false })

    await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, true)
    .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts(refreshingBorrowerRemainingAccounts)
    .signers([borrowerWalletKeypair])
    .rpc()

    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)

    const supplierLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))

    const borrowerLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    console.log("Token Reserve Supply Interest Change Index: ", Number(tokenReserve.supplyInterestChangeIndex))
    console.log("Token Reserve Borrow Interest Change Index: ", Number(tokenReserve.borrowInterestChangeIndex))
    console.log("Token Reserve Interest Earned: ", Number(tokenReserve.interestEarnedAmount))
    console.log("Token Reserve Interest Accrued: ", Number(tokenReserve.interestAccruedAmount))
    console.log("Token Reserve SolvencyFees: ", Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount))
    console.log("Token Reserve Balance After Repayment: ", tokenReserveUSDCATABalance.value.uiAmount, "\n")

    var solSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(solTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    var usdcSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    console.log("SOL SubMarketFees: ", Number(solSubMarket.subMarketFeesGeneratedAmount))
    console.log("USDC SubMarketFees: ", Number(usdcSubMarket.subMarketFeesGeneratedAmount))

    console.log("Supplier Interest Earned: ", Number(supplierLendingUserTabAccount.interestEarnedAmount))
    console.log("Supplier Interest Accrued: ", Number(supplierLendingUserTabAccount.interestAccruedAmount))
    console.log("Supplier Fees Generated: ", Number(supplierLendingUserTabAccount.feesGeneratedAmount))

    console.log("Borrower Interest Earned: ", Number(borrowerLendingUserTabAccount.interestEarnedAmount))
    console.log("Borrower Interest Accrued: ", Number(borrowerLendingUserTabAccount.interestAccruedAmount))
    console.log("Borrower Fees Generated: ", Number(borrowerLendingUserTabAccount.feesGeneratedAmount))
  })

  it("Verifies you can't Over Repay.", async () => 
  {
    var errorMessage = ""

    try
    {
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, borrowerWalletKeypair.publicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        borrowerSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        borrowerSOLMonthlyStatementRemainingAccount,

        borrowerUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        borrowerUSDCMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
      .signers([borrowerWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()
      
      const repayTokenInstruction = await program.methods.repayTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      overBorrowUSDCAmount,
      false,
      false)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: borrowerWalletKeypair.publicKey })
      .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
      .signers([borrowerWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, repayTokenInstruction], [borrowerWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.tooManyFundsErrorMsg))
  })

  it("Repays Borrowed USDC To the Token Reserve", async () => 
  {
    var usdcSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    var tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    var tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    var tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)
    var currentTokenReserveAmount = Number((Number(tokenReserve.depositedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount) -
    Number(tokenReserve.borrowedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount) +
    Number(usdcSubMarket.subMarketFeesGeneratedAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount) +
    Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount) / Math.pow(10, tokenReserve.tokenDecimalAmount)).toFixed(tokenReserve.tokenDecimalAmount))
    
    if(tokenReserveUSDCATABalance.value.uiAmount)
      assert(tokenReserveUSDCATABalance.value.uiAmount >= currentTokenReserveAmount)
    else
      throw new Error("tokenReserveUSDCATABalance.value.uiAmount undefined")

    await closeUserPreviousTempOraclePriceDataAccount(borrowerWalletKeypair.publicKey, borrowerWalletKeypair)
    const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, borrowerWalletKeypair.publicKey)

    const refreshingRemainingAccounts =
    [
      //Price Account
      priceRemainingAccount,

      //Token Reserves
      solTokenReserveRemainingAccount,
      usdcTokenReserveRemainingAccount,

      //Sets of Tabs, Submarkets, and Monthly Statement Accounts
      borrowerSOLLendingUserTabRemainingAccount,
      solSubMarketRemainingAccount,
      borrowerSOLMonthlyStatementRemainingAccount,

      borrowerUSDCLendingUserTabRemainingAccount,
      usdcSubMarketRemainingAccount,
      borrowerUSDCMonthlyStatementRemainingAccount
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
    .accounts({ lendingUserOwner: borrowerWalletKeypair.publicKey, signer: borrowerWalletKeypair.publicKey })
    .signers([borrowerWalletKeypair])
    .remainingAccounts(refreshingRemainingAccounts)
    .instruction()

    const repayTokenInstruction = await program.methods.repayTokens(
    testSubMarketIndex,
    testUserAccountIndex,
    lessThan10PercentOfBorrowedAmount,
    true,
    false)
    .accounts({
      subMarketOwner: programProviderPublicKey,
      tokenMint: usdcMint.publicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      signer: borrowerWalletKeypair.publicKey })
    .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
    .signers([borrowerWalletKeypair])
    .instruction()

    await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
    await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, repayTokenInstruction], [borrowerWalletKeypair])

    var tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    assert(tokenReserve.borrowedAmount.eq(bnZero))

    const borrowerLendingUserAccount = await program.account.lendingUserAccount.fetch(getLendingUserAccountPDA
    (
      borrowerWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(borrowerLendingUserAccount.totalBorrowedUsdValue.eq(bnZero))

    const borrowerLendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, successorWalletKeypair.publicKey)

      const refreshingRemainingAccounts =
      [
        //Price Account
        priceRemainingAccount,

        //Token Reserves
        solTokenReserveRemainingAccount,
        usdcTokenReserveRemainingAccount,

        //Sets of Tabs, Submarkets, and Monthly Statement Accounts
        supplierUSDCLendingUserTabRemainingAccount,
        usdcSubMarketRemainingAccount,
        supplierUSDCMonthlyStatementRemainingAccount,
        
        supplierSOLLendingUserTabRemainingAccount,
        solSubMarketRemainingAccount,
        supplierSOLMonthlyStatementRemainingAccount
      ]

      const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
      .accounts({ lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .remainingAccounts(refreshingRemainingAccounts)
      .instruction()

      const withdrawInstruction = await program.methods.withdrawTokens(
        testSubMarketIndex,
        testUserAccountIndex,
        borrowerUSDCAmount,
        true)
      .accounts({
        subMarketOwner: programProviderPublicKey,
        tokenMint: usdcMint.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .instruction()

      await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
      await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, withdrawInstruction], [successorWalletKeypair])
    }
    catch(error: any)
    {
      errorMessage = error.transactionLogs.toString()
    }

    assert(errorMessage.includes(errors.incorrectOrderOfTabAccountsErrorMsg))
  })

  it("Withdraws USDC From the Token Reserve", async () => 
  {
    await closeUserPreviousTempOraclePriceDataAccount(successorWalletKeypair.publicKey, successorWalletKeypair)
    const [updatePricesTransaction, priceRemainingAccount] = await generateOracleTransactionAndRemainingPriceAccount(solLiquidatePriceWithUSDCDataPayload, successorWalletKeypair.publicKey)

    const refreshingRemainingAccounts =
    [
      //Price Account
      priceRemainingAccount,

      //Token Reserves
      solTokenReserveRemainingAccount,
      usdcTokenReserveRemainingAccount,

      //Sets of Tabs, Submarkets, and Monthly Statement Accounts
      supplierSOLLendingUserTabRemainingAccount,
      solSubMarketRemainingAccount,
      supplierSOLMonthlyStatementRemainingAccount,

      supplierUSDCLendingUserTabRemainingAccount,
      usdcSubMarketRemainingAccount,
      supplierUSDCMonthlyStatementRemainingAccount  
    ]

    const refreshUserHealthAndTokenReservesInstruction = await program.methods.refreshUserHealthChunkAndTokenReserves(testUserAccountIndex, 2, 2, false)
    .accounts({ lendingUserOwner: successorWalletKeypair.publicKey, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .remainingAccounts(refreshingRemainingAccounts)
    .instruction()
    
    const withdrawInstruction = await program.methods.withdrawTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      borrowerUSDCAmount,
      true)
    .accounts({
      subMarketOwner: programProviderPublicKey,
      tokenMint: usdcMint.publicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      signer: successorWalletKeypair.publicKey })
    .remainingAccounts([priceRemainingAccount, oracleAddressRemainingAccount])
    .signers([successorWalletKeypair])
    .instruction()

    await program.provider.connection.sendRawTransaction(updatePricesTransaction.serialize(), { skipPreflight: false })
    await sendVersionedTrasaction([refreshUserHealthAndTokenReservesInstruction, withdrawInstruction], [successorWalletKeypair])

    var lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      successorWalletKeypair.publicKey,
      testUserAccountIndex
    ))
    assert(lendingUserTabAccount.owner.toBase58() == successorWalletKeypair.publicKey.toBase58())
    assert(lendingUserTabAccount.userAccountIndex == testUserAccountIndex)
    assert(lendingUserTabAccount.tokenId == usdcTestPriceDataPayload.data[0].tokenId)
    assert(lendingUserTabAccount.subMarketOwnerAddress.toBase58() == programProviderPublicKeyString)
    assert(lendingUserTabAccount.subMarketIndex == testSubMarketIndex)
    assert(lendingUserTabAccount.userTabAccountIndex == 1)
    assert(lendingUserTabAccount.userTabAccountAdded == true)
    assert(lendingUserTabAccount.depositedAmount.eq(bnZero))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    assert(tokenReserve.tokenId == 2)
    assert(tokenReserve.tokenMintAddress.toBase58() == usdcMint.publicKey.toBase58())
    assert(tokenReserve.tokenDecimalAmount == usdcTokenDecimalAmount)
    assert(tokenReserve.depositedAmount.eq(bnZero))
    console.log("Token Reserve Interest Earned: ", Number(tokenReserve.interestEarnedAmount))
    console.log("Token Reserve Interest Accrued: ", Number(tokenReserve.interestAccruedAmount))
    console.log("SubMarket Fees Generated: ", Number(subMarket.subMarketFeesGeneratedAmount))
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
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
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
      testSubMarketIndex,
      testUserAccountIndex,
      null,
      null
      )
      .accounts({ tokenMintAddress: usdcMint.publicKey, subMarketOwner: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notFeeCollectorErrorMsg)
  })

  it("Claims SubMarket Fees", async () => 
  {
    await program.methods.claimSubMarketFees(
    testSubMarketIndex,
    testUserAccountIndex,
    null,
    null
    )
    .accounts({ tokenMintAddress: usdcMint.publicKey, subMarketOwner: programProviderPublicKey })
    .rpc()

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(usdcMint.publicKey))
    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      programProviderPublicKey,
      testUserAccountIndex
    ))
    
    //Claiming SubMarket Fees just puts it in the Fee Collector's Tab Account
    assert(parseInt(tokenReserveUSDCATABalance.value.amount) >= Number(lendingUserTabAccount.depositedAmount) + Number(tokenReserve.uncollectedSolvencyInsuranceFeesAmount))

    const subMarket = await program.account.subMarket.fetch(getSubMarketPDA(usdcTestPriceDataPayload.data[0].tokenId, programProviderPublicKey, testSubMarketIndex))
    assert(subMarket.uncollectedSubMarketFeesAmount.eq(bnZero))
  })

  it("Verifies only Solvency Treasurer can Collect Solvency Insurance Fees", async () => 
  {
    var errorMessage = ""

    try
    {
      await program.methods.claimSolvencyInsuranceFees(
      testSubMarketIndex,
      testUserAccountIndex,
      null,
      null
      )
      .accounts({
        tokenMint: usdcMint.publicKey,
        subMarketOwner: programProviderPublicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notSolvencyTreasurerErrorMsg)
  })

  it("Claims Token Reserve Solvency Insurance Fees", async () => 
  {
    await program.methods.claimSolvencyInsuranceFees(
    testSubMarketIndex,
    testUserAccountIndex,
    null,
    null
    )
    .accounts({ tokenMint: usdcMint.publicKey, subMarketOwner: programProviderPublicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    const tokenReserveUSDCATA = await deriveATA(getTokenReservePDA(usdcMint.publicKey), usdcMint.publicKey, true)
    const tokenReserveUSDCATABalance = await program.provider.connection.getTokenAccountBalance(tokenReserveUSDCATA)

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      usdcTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      programProviderPublicKey,
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
      testSubMarketIndex,
      testUserAccountIndex,
      null,
      null
      )
      .accounts({ tokenMintAddress: usdcMint.publicKey, subMarketOwner: programProviderPublicKey, signer: successorWalletKeypair.publicKey })
      .signers([successorWalletKeypair])
      .rpc()
    }
    catch(error: any)
    {
      errorMessage = error.error.errorMessage
    }

    assert(errorMessage == errors.notLiquidationTreasurerErrorMsg)
  })

  it("Claims Token Reserve Liquidation Fees", async () => 
  {
    await program.methods.claimLiquidationFees(
    testSubMarketIndex,
    testUserAccountIndex,
    null,
    null
    )
    .accounts({ tokenMintAddress: solTokenMintAddress, subMarketOwner: programProviderPublicKey })
    .rpc()

    const lendingUserTabAccount = await program.account.lendingUserTabAccount.fetch(getLendingUserTabAccountPDA
    (
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      programProviderPublicKey,
      testUserAccountIndex
    ))
    const lendingUserMonthlyStatementAccount = await program.account.lendingUserMonthlyStatementAccount.fetch(getlendingUserMonthlyStatementAccountPDA
    (
      newStatementMonth,
      newStatementYear,
      solTestPriceDataPayload.data[0].tokenId,
      programProviderPublicKey,
      testSubMarketIndex,
      programProviderPublicKey,
      testUserAccountIndex
    ))

    assert(lendingUserTabAccount.feesGeneratedAmount.gt(bnZero))
    assert(lendingUserTabAccount.feesGeneratedAmount.eq(lendingUserMonthlyStatementAccount.monthlyLiquidationFeesCollectedAmount))

    const tokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(solTokenMintAddress))
    assert(tokenReserve.uncollectedLiquidationFeesAmount.eq(bnZero))
  })

  it("Adds a DAI, WEth, and WBtc Token Reserves", async () => 
  {
    await program.methods.addTokenReserve(daiTokenDecimalAmount, baseBorrowAPY, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate4Percent)
    .accounts({ tokenMint: daiMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    await program.methods.addTokenReserve(wethTokenDecimalAmount, baseBorrowAPY, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate4Percent)
    .accounts({ tokenMint: wethMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    await program.methods.addTokenReserve(wbtcTokenDecimalAmount, baseBorrowAPY, useUSDCFixedBorrowAPY, globalLimit1, solvencyInsuranceFeeRate4Percent)
    .accounts({ tokenMint: wbtcMint.publicKey, tokenProgram: TOKEN_2022_PROGRAM_ID })
    .rpc()

    const daiTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(daiMint.publicKey))
    assert(daiTokenReserve.tokenId == 3)
    assert(daiTokenReserve.tokenMintAddress.toBase58() == daiMint.publicKey.toBase58())
    assert(daiTokenReserve.tokenDecimalAmount == daiTokenDecimalAmount)
    assert(daiTokenReserve.depositedAmount.eq(bnZero))
    assert(daiTokenReserve.borrowApy == baseBorrowAPY)
    assert(daiTokenReserve.globalLimit.eq(globalLimit1))
    assert(daiTokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate4Percent)

    const wethTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(wethMint.publicKey))
    assert(wethTokenReserve.tokenId == 4)
    assert(wethTokenReserve.tokenMintAddress.toBase58() == wethMint.publicKey.toBase58())
    assert(wethTokenReserve.tokenDecimalAmount == wethTokenDecimalAmount)
    assert(wethTokenReserve.depositedAmount.eq(bnZero))
    assert(wethTokenReserve.borrowApy == baseBorrowAPY)
    assert(wethTokenReserve.globalLimit.eq(globalLimit1))
    assert(wethTokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate4Percent)

    const wbtcTokenReserve = await program.account.tokenReserve.fetch(getTokenReservePDA(wbtcMint.publicKey))
    assert(wbtcTokenReserve.tokenId == 5)
    assert(wbtcTokenReserve.tokenMintAddress.toBase58() == wbtcMint.publicKey.toBase58())
    assert(wbtcTokenReserve.tokenDecimalAmount == wbtcTokenDecimalAmount)
    assert(wbtcTokenReserve.depositedAmount.eq(bnZero))
    assert(wbtcTokenReserve.borrowApy == baseBorrowAPY)
    assert(wbtcTokenReserve.globalLimit.eq(globalLimit1))
    assert(wbtcTokenReserve.solvencyInsuranceFeeRate == solvencyInsuranceFeeRate4Percent)

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
    await addAddressToLookUpTable(protocolLookUpTableAddress, daiTokenReservePDA, "DAI Token Reserve")
    await addAddressToLookUpTable(protocolLookUpTableAddress, wethTokenReservePDA, "WEth Token Reserve")
    await addAddressToLookUpTable(protocolLookUpTableAddress, wbtcTokenReservePDA, "WBtc Token Reserve")

    //Get latest lookup table
    protocolLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(protocolLookUpTableAddress)).value
  })

  it("Creates a DAI, WEth, and WBtc SubMarket", async () => 
  {
    await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRate8Percent, null)
    .accounts({ tokenMintAddress: daiMint.publicKey, feeCollectorAddress: programProviderPublicKey }).rpc()
    await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRate8Percent, null)
    .accounts({ tokenMintAddress: wethMint.publicKey, feeCollectorAddress: programProviderPublicKey }).rpc()
    await program.methods.createSubMarket(testSubMarketIndex, subMarketFeeRate8Percent, null)
    .accounts({ tokenMintAddress: wbtcMint.publicKey, feeCollectorAddress: programProviderPublicKey }).rpc()

    const daiSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(3, programProviderPublicKey, testSubMarketIndex))
    assert(daiSubMarket.owner.toBase58() == programProviderPublicKeyString)
    assert(daiSubMarket.feeCollectorAddress.toBase58() == programProviderPublicKeyString)
    assert(daiSubMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(daiSubMarket.tokenId == 3)
    assert(daiSubMarket.subMarketIndex == testSubMarketIndex)

    const wethSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(4, programProviderPublicKey, testSubMarketIndex))
    assert(wethSubMarket.owner.toBase58() == programProviderPublicKeyString)
    assert(wethSubMarket.feeCollectorAddress.toBase58() == programProviderPublicKeyString)
    assert(wethSubMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(wethSubMarket.tokenId == 4)
    assert(wethSubMarket.subMarketIndex == testSubMarketIndex)

    const wbtcSubMarket = await program.account.subMarket.fetch(getSubMarketPDA(5, programProviderPublicKey, testSubMarketIndex))
    assert(wbtcSubMarket.owner.toBase58() == programProviderPublicKeyString)
    assert(wbtcSubMarket.feeCollectorAddress.toBase58() == programProviderPublicKeyString)
    assert(wbtcSubMarket.feeOnInterestEarnedRate == subMarketFeeRate8Percent)
    assert(wbtcSubMarket.tokenId == 5)
    assert(wbtcSubMarket.subMarketIndex == testSubMarketIndex)

    //Populate DAI SubMarket Remaining Account
    const daiSubMarketPDA = getSubMarketPDA(3, programProviderPublicKey, testSubMarketIndex)
    daiSubMarketRemainingAccount = 
    {
      pubkey: daiSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate WEth SubMarket Remaining Account
    const wethSubMarketPDA = getSubMarketPDA(4, programProviderPublicKey, testSubMarketIndex)
    wethSubMarketRemainingAccount = 
    {
      pubkey: wethSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Populate WBtc SubMarket Remaining Account
    const wbtcSubMarketPDA = getSubMarketPDA(5, programProviderPublicKey, testSubMarketIndex)
    wbtcSubMarketRemainingAccount = 
    {
      pubkey: wbtcSubMarketPDA,
      isSigner: false,
      isWritable: true
    }

    //Add SubMarkets to Address Lookup Table
    await addAddressToLookUpTable(mainSubMarketOwnerLookUpTableAddress, daiSubMarketPDA, "DAI SubMarket")
    await addAddressToLookUpTable(mainSubMarketOwnerLookUpTableAddress, wethSubMarketPDA, "WEth SubMarket")
    await addAddressToLookUpTable(mainSubMarketOwnerLookUpTableAddress, wbtcSubMarketPDA, "WBtc SubMarket")

    //Get latest lookup table
    mainSubMarketOwnerLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(mainSubMarketOwnerLookUpTableAddress)).value
  })

  it("Deposits SOL, USDC, DAI, WEth, BTC into Token Reserve", async () => 
  {
    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, twoSol, null, null)
    .accounts({ tokenMint: solTokenMintAddress, subMarketOwner: programProviderPublicKey, tokenProgram: TOKEN_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()
    
    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, supplierUSDCAmount, null, null)
    .accounts({ tokenMint: usdcMint.publicKey, subMarketOwner: programProviderPublicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, daiDepositAmount, null, null)
    .accounts({ tokenMint: daiMint.publicKey, subMarketOwner: programProviderPublicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, wethDepositAmount, null, null)
    .accounts({ tokenMint: wethMint.publicKey, subMarketOwner: programProviderPublicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    await program.methods.depositTokens(testSubMarketIndex, testUserAccountIndex, wbtcDepositAmount, null, null)
    .accounts({ tokenMint: wbtcMint.publicKey, subMarketOwner: programProviderPublicKey, tokenProgram: TOKEN_2022_PROGRAM_ID, signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .rpc()

    //Populate Supplier DAI Tab Remaining Account
    const successorDAILendingUserTabAccountPDA = getLendingUserTabAccountPDA
    (
      3,
      programProviderPublicKey,
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
      4,
      programProviderPublicKey,
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
      5,
      programProviderPublicKey,
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
      3,
      programProviderPublicKey,
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
      4,
      programProviderPublicKey,
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
      5,
      programProviderPublicKey,
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

    //Add Lending User Tab and Monthly Statment Accounts to Address Lookup Table
    await addAddressToLookUpTable(
      supplierLookUpTableAddress,
      [
        successorDAILendingUserTabAccountPDA, supplierDAIMonthlyStatementPDA,
        successorWEthLendingUserTabAccountPDA, supplierWEthMonthlyStatementPDA,
        successorWBtcLendingUserTabAccountPDA, supplierWBtcMonthlyStatementPDA,
      ],
      "Lending User Tab and Monthly Statement")

    //Get latest lookup table
    supplierLookUpTableAccount = (await program.provider.connection.getAddressLookupTable(supplierLookUpTableAddress)).value
  })

  it("Withdraws DAI, WEth, and WBtc From the Token Reserve", async () => 
  {
    const refreshingRemainingAccounts =
    [
      //Token Reserves
      solTokenReserveRemainingAccount,
      usdcTokenReserveRemainingAccount,
      daiTokenReserveRemainingAccount,
      wethTokenReserveRemainingAccount,
      wbtcTokenReserveRemainingAccount,

      //Sets of Tabs, Submarkets, and Monthly Statement Accounts
      supplierSOLLendingUserTabRemainingAccount,
      solSubMarketRemainingAccount,
      supplierSOLMonthlyStatementRemainingAccount,

      supplierUSDCLendingUserTabRemainingAccount,
      usdcSubMarketRemainingAccount,
      supplierUSDCMonthlyStatementRemainingAccount,

      supplierDAILendingUserTabRemainingAccount,
      daiSubMarketRemainingAccount,
      supplierDAIMonthlyStatementRemainingAccount,

      supplierWEthLendingUserTabRemainingAccount,
      wethSubMarketRemainingAccount,
      supplierWEthMonthlyStatementRemainingAccount,

      supplierWBtcLendingUserTabRemainingAccount,
      wbtcSubMarketRemainingAccount,
      supplierWBtcMonthlyStatementRemainingAccount
    ]

    const withdrawDAIInstruction = await program.methods.withdrawTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      daiHalfDepositAmount,
      false)
    .accounts({
      subMarketOwner: programProviderPublicKey,
      tokenMint: daiMint.publicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .instruction()

    const withdrawWEthInstruction = await program.methods.withdrawTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      wethHalfDepositAmount,
      false)
    .accounts({
      subMarketOwner: programProviderPublicKey,
      tokenMint: wethMint.publicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .instruction()

    const withdrawWBtcInstruction = await program.methods.withdrawTokens(
      testSubMarketIndex,
      testUserAccountIndex,
      wbtcHalfDepositAmount,
      false)
    .accounts({
      subMarketOwner: programProviderPublicKey,
      tokenMint: wbtcMint.publicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      signer: successorWalletKeypair.publicKey })
    .signers([successorWalletKeypair])
    .instruction()

    await sendVersionedTrasaction([withdrawDAIInstruction], [successorWalletKeypair])
    await sendVersionedTrasaction([withdrawWEthInstruction], [successorWalletKeypair])
    await sendVersionedTrasaction([withdrawWBtcInstruction], [successorWalletKeypair])
  })

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
    transaction.sign(walletKeyPair)
    //const signedTransaction = await programProvider.wallet.signTransaction(transaction)

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
        programProviderPublicKey,
        [testingWalletKeypair],
        mintAmount
      )
    )

    //2. Send the transaction
    await programProvider.sendAndConfirm(transaction)
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
    console.log(`${timeLeftInSeconds} Timeout Seconds Left`)

    const countDownIntervalId = setInterval(() =>
    {
      timeLeftInSeconds -= 10
      if(timeLeftInSeconds > 0)
        console.log(`${timeLeftInSeconds} Timeout Seconds Left`)
      
      if(timeLeftInSeconds <= 0)
        clearInterval(countDownIntervalId)  
    }, 10000) 
  }

  async function initLookUpTable()
  {
    console.log("Creating Lookup Table")

    const slot = await program.provider.connection.getSlot()

    const [createInstruction, lookUpTableAddress] = 
    AddressLookupTableProgram.createLookupTable({
      authority: programProviderPublicKey,
      payer: programProviderPublicKey,
      recentSlot: slot,
    })

    await programProvider.sendAndConfirm(new Transaction().add(createInstruction), [])
    await timeOutFunction(1)

    return lookUpTableAddress
  }

  async function addAddressToLookUpTable(lookUpTableAddress: PublicKey, newAddresses: PublicKey | PublicKey[], accountDescription: string)
  {
    console.log(`Adding ${accountDescription} Address(s) to Lookup Table`)

    const addressesToAdd = Array.isArray(newAddresses) ? newAddresses : [newAddresses]

    const extendInstruction = AddressLookupTableProgram.extendLookupTable(
    {
      payer: programProvider.wallet.publicKey,
      authority: programProvider.wallet.publicKey,
      lookupTable: lookUpTableAddress,
      addresses: addressesToAdd
    })

    await programProvider.sendAndConfirm(new Transaction().add(extendInstruction))

    await timeOutFunction(1)
  }

  async function generateOracleTransactionAndRemainingPriceAccount(payload: PriceDataPayload, lendingUserAdress: PublicKey): Promise<[Transaction, AccountMeta]>
  {
    let transaction = new Transaction
    const latestBlockhash = await program.provider.connection.getLatestBlockhash()
    transaction.recentBlockhash = latestBlockhash.blockhash
    transaction.feePayer = priceValidatorKeypair.publicKey

    payload.slot = new anchor.BN(await program.provider.connection.getSlot())

    const createTempOraclePriceDataInstruction = await program.methods.createTempOraclePriceData(payload)
      .accounts({ lendingUserAddress: lendingUserAdress, signer: priceValidatorKeypair.publicKey })
      .signers([priceValidatorKeypair])
      .instruction()
    
    transaction.add(createTempOraclePriceDataInstruction)

    const priceAccountPDA = getPriceAccountPDA(lendingUserAdress)
    const priceRemainingAccount = 
    {
      pubkey: priceAccountPDA,
      isSigner: false,
      isWritable: true
    }
      
    transaction.sign(priceValidatorKeypair)
    
    //const signedTransaction = await program.provider.wallet?.signTransaction(transaction)

    //if(!signedTransaction)
      //throw new Error("Failed to Sign Transaction")

    return [transaction, priceRemainingAccount]
  }

  async function sendVersionedTrasaction(instructions: anchor.web3.TransactionInstruction[], signerKeypair: Keypair[])
  {
    const { blockhash } = await program.provider.connection.getLatestBlockhash()

    var lookUpTableArray = []

    if(protocolLookUpTableAccount)
      lookUpTableArray.push(protocolLookUpTableAccount as anchor.web3.AddressLookupTableAccount)
    if(mainSubMarketOwnerLookUpTableAccount)
      lookUpTableArray.push(mainSubMarketOwnerLookUpTableAccount as anchor.web3.AddressLookupTableAccount)
    if(borrowerLookUpTableAccount)
      lookUpTableArray.push(borrowerLookUpTableAccount as anchor.web3.AddressLookupTableAccount)
    if(supplierLookUpTableAccount)
      lookUpTableArray.push(supplierLookUpTableAccount as anchor.web3.AddressLookupTableAccount)

    const messageV0 = new TransactionMessage(
    {
      payerKey: programProviderPublicKey,
      recentBlockhash: blockhash,
      instructions: instructions
    }).compileToV0Message(lookUpTableArray)

    //Create Versioned Transaction
    const transaction = new VersionedTransaction(messageV0)

    const size = transaction.serialize().length
    console.log(`Transaction Size: ${size} bytes`)

    await programProvider.sendAndConfirm(transaction, signerKeypair)
  }

  async function sendVersionedTrasactionWithHigherCompute(instructions: anchor.web3.TransactionInstruction[], signerKeypair: Keypair[])
  {
    const { blockhash } = await program.provider.connection.getLatestBlockhash()

    var lookUpTableArray = []

    if(protocolLookUpTableAccount)
      lookUpTableArray.push(protocolLookUpTableAccount as anchor.web3.AddressLookupTableAccount)
    if(mainSubMarketOwnerLookUpTableAccount)
      lookUpTableArray.push(mainSubMarketOwnerLookUpTableAccount as anchor.web3.AddressLookupTableAccount)
    if(borrowerLookUpTableAccount)
      lookUpTableArray.push(borrowerLookUpTableAccount as anchor.web3.AddressLookupTableAccount)
    if(supplierLookUpTableAccount)
      lookUpTableArray.push(supplierLookUpTableAccount as anchor.web3.AddressLookupTableAccount)

    const modifyComputeUnits = anchor.web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 })
    const finalizedInstructions = [modifyComputeUnits, ...instructions]

    const messageV0 = new TransactionMessage(
    {
      payerKey: programProviderPublicKey,
      recentBlockhash: blockhash,
      instructions: finalizedInstructions
    }).compileToV0Message(lookUpTableArray)

    //Create Versioned Transaction
    const transaction = new VersionedTransaction(messageV0)

    const size = transaction.serialize().length
    console.log(`Transaction Size: ${size} bytes`)

    await programProvider.sendAndConfirm(transaction, signerKeypair)
  }

  async function closeUserPreviousTempOraclePriceDataAccount(userPublicKey: PublicKey, userKeyPair: Keypair)
  {
    //Close user previous temp oracle price data account
    await program.methods.closeTempOraclePriceData()
      .accounts({ signer: userPublicKey })
      .remainingAccounts([oracleAddressRemainingAccount])
      .signers([userKeyPair])
      .rpc()
  }
})