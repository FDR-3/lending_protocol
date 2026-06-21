import * as anchor from "@coral-xyz/anchor"
import { PublicKey } from '@solana/web3.js'
import idl from "../target/idl/lending_protocol.json";

const programId = new PublicKey(idl.address);


export function getLendingProtocolPDA()
{
  const [lendingProtocolCEOPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("lendingProtocol")
    ],
    programId
  )
  return lendingProtocolCEOPDA
}

export function getLendingStatsPDA()
{
  const [lendingStatsPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("lendingStats")
    ],
    programId
  )
  return lendingStatsPDA
}

export function getLendingUserStatsPDA()
{
  const [lendingUserStatsPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("lendingUserStats")
    ],
    programId
  )
  return lendingUserStatsPDA
}

export function getTokenReserveStatsPDA()
{
  const [tokenReserveStatsPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("tokenReserveStats")
    ],
    programId
  )
  return tokenReserveStatsPDA
}

export function getSubMarketStatsPDA()
{
  const [subMarketPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("subMarketStats")
    ],
    programId
  )
  return subMarketPDA
}

export function getLendingProtocolCEOPDA()
{
  const [lendingProtocolCEOPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("lendingProtocolCEO")
    ],
    programId
  )
  return lendingProtocolCEOPDA
}

export function getSolvencyTreasurerPDA()
{
  const [solvencyTreasurerPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("solvencyTreasurer")
    ],
    programId
  )
  return solvencyTreasurerPDA
}

export function getLiquidationTreasurerPDA()
{
  const [liquidationTreasurerPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("liquidationTreasurer")
    ],
    programId
  )
  return liquidationTreasurerPDA
}

export function getOraclePriceValidatorPDA()
{
  const [oraclePriceValidatorPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("oraclePriceValidator")
    ],
    programId
  )
  return oraclePriceValidatorPDA
}

export function getPriceAccountPDA(lendingUserAddress: PublicKey)
{
  const [oraclePriceAccountPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("oraclePriceData"),
      lendingUserAddress.toBuffer()
    ],
    programId
  )
  return oraclePriceAccountPDA
}

export function getTokenReservePDA(tokenMintAddress: PublicKey)
{
  const [tokenReservePDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("tokenReserve"),
      tokenMintAddress.toBuffer()

    ],
    programId
  )
  return tokenReservePDA
}

export function getSubMarketPDA(tokenId: number, subMarketOwner: PublicKey, subMarketIndex: number)
{
  const [subMarketPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("subMarket"),
      new anchor.BN(tokenId).toBuffer('le', 1),
      subMarketOwner.toBuffer(),
      new anchor.BN(subMarketIndex).toBuffer('le', 2)
    ],
    programId
  )
  return subMarketPDA
}

export function getLendingUserAccountPDA(lendingUserAddress: PublicKey, lendingUserAccountIndex: number)
{
  const [lendingUserTabAccountPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("lendingUserAccount"),
      lendingUserAddress.toBuffer(),
      new anchor.BN(lendingUserAccountIndex).toBuffer('le', 1),
    ],
    programId
  )
  return lendingUserTabAccountPDA
}

export function getLendingUserTabAccountPDA(tokenId: number,
  subMarketOwner: PublicKey,
  subMarketIndex: number,
  lendingUserAddress: PublicKey,
  lendingUserAccountIndex: number)
{
  const [lendingUserTabAccountPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("lendingUserTabAccount"),
      new anchor.BN(tokenId).toBuffer('le', 1),
      subMarketOwner.toBuffer(),
      new anchor.BN(subMarketIndex).toBuffer('le', 2),
      lendingUserAddress.toBuffer(),
      new anchor.BN(lendingUserAccountIndex).toBuffer('le', 1),
    ],
    programId
  )
  return lendingUserTabAccountPDA
}

export function getlendingUserMonthlyStatementAccountPDA(statementMonth: number,
  statementYear: number,
  tokenId: number,
  subMarketOwnerAddress: PublicKey,
  subMarketIndex: number,
  lendingUserAddress: PublicKey,
  lendingUserAccountIndex: number)
{
  const [lendingUserMonthlyStatementAccountPDA] = PublicKey.findProgramAddressSync
  (
    [
      new TextEncoder().encode("userMonthlyStatementAccount"),//lendingUserMonthlyStatementAccount was too long, can only be 32 characters, lol
      new anchor.BN(statementMonth).toBuffer('le', 1),
      new anchor.BN(statementYear).toBuffer('le', 2),
      new anchor.BN(tokenId).toBuffer('le', 1),
      subMarketOwnerAddress.toBuffer(),
      new anchor.BN(subMarketIndex).toBuffer('le', 2),
      lendingUserAddress.toBuffer(),
      new anchor.BN(lendingUserAccountIndex).toBuffer('le', 1),
    ],
    programId
  )
  return lendingUserMonthlyStatementAccountPDA
}