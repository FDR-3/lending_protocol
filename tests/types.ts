import * as anchor from "@coral-xyz/anchor"

export type PriceDataPayload =
{
    data: VerifiedPriceData[]
    slot: anchor.BN
}

export type VerifiedPriceData =
{
  tokenId: number; //u8
  normalizedPrice18Decimals: anchor.BN //u128
}