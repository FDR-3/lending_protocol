import * as anchor from "@coral-xyz/anchor"
import { Program } from "@coral-xyz/anchor"
import { LendingProtocol } from "../target/types/lending_protocol"

describe("lending_protocol", () =>
{
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env())

  const program = anchor.workspace.LendingProtocol as Program<LendingProtocol>

  it("Initializes Lending Protocol", async () => 
  {
    const tx = await program.methods.initializeLendingProtocol().rpc()
  })
})
