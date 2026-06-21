import * as anchor from "@coral-xyz/anchor"
import { runInsolventTest } from "./test_settings"
import type { PriceDataPayload } from "./types"

export const solTestPriceDataPayload: PriceDataPayload = 
{
  data: 
  [{
    tokenId: 1,
    normalizedPrice18Decimals: new anchor.BN("100000000000000000000") //$100.00 USD
  }],
  slot: new anchor.BN(0)
}

export const solAndUSDCTestPriceDataPayload: PriceDataPayload = 
{
  data: 
  [{
    tokenId: 1,
    normalizedPrice18Decimals: new anchor.BN("100000000000000000000") //$100.00 USD
  },
  {
    tokenId: 2,
    normalizedPrice18Decimals: new anchor.BN("1000000000000000000") //$1.00 USD
  }],
  slot: new anchor.BN(0)
}

export const solCantLiquidatePriceDataPayload: PriceDataPayload =
{
  data: 
  [{
    tokenId: 1,
    normalizedPrice18Decimals: new anchor.BN("875000001000000000000") //$875.000001 USD
  }],
  slot: new anchor.BN(0)
}

export var solLiquidatePriceWithUSDCDataPayload: PriceDataPayload
if(!runInsolventTest)
{
  solLiquidatePriceWithUSDCDataPayload =
  {
    data: 
    [{
      tokenId: 1,
      normalizedPrice18Decimals: new anchor.BN("87500000000000000000") //$87.50 USD
    },
    {
      tokenId: 2,
      normalizedPrice18Decimals: new anchor.BN("1000000000000000000") //$1.00 USD
    }],
    slot: new anchor.BN(0)
  }
}
else
{
  solLiquidatePriceWithUSDCDataPayload =
  {
    data: 
    [{
      tokenId: 1,
      normalizedPrice18Decimals: new anchor.BN("70000000000000000000") //$70.00 USD
    },
    {
      tokenId: 2,
      normalizedPrice18Decimals: new anchor.BN("1000000000000000000") //$1.00 USD
    }],
    slot: new anchor.BN(0)
  }
}

export const usdcTestPriceDataPayload: PriceDataPayload = 
{
  data: 
  [{
    tokenId: 2,
    normalizedPrice18Decimals: new anchor.BN("1000000000000000000") //$1.00 USD
  }],
  slot: new anchor.BN(0)
}