# Apps & LAM Integration

This guide explains how to build applications on top of the Origin OS Protocol and integrate with the LAM (Language AI Model) interface for ChatGPT apps.

## Overview

The Origin OS Protocol is designed to be used through AI-powered applications that simplify the user experience. Users interact with the protocol by chatting with a LAM inside ChatGPT, which guides them through:

- Finding and selecting hosting providers
- Opening sessions with escrow and insurance
- Monitoring session status
- Filing claims when needed
- Managing collateral (for providers)

## What is the LAM?

The **LAM** (Language AI Model interface) is the AI helper that users talk to inside ChatGPT apps. It acts as a conversational interface to the Origin OS Protocol, translating natural language requests into protocol actions.

**Example Interactions**:
- User: *"Find me the fastest host near San Francisco"*
- LAM: Queries provider registry, ranks by latency, presents options
- User: *"Open a session with Provider X for 100 GB storage"*
- LAM: Calculates insurance, builds transaction, prompts for wallet signature

## Architecture

### Application Components

Each Origin OS app consists of two main components:

#### 1. UI Widget (Frontend)
- React-based interface rendered inside ChatGPT
- Displays:
  - Provider listings and stats
  - Session details
  - Transaction confirmations
  - Claim status
- Handles wallet interactions (signature requests)

#### 2. MCP Server (Backend)
- Model Context Protocol (MCP) server exposing tools and resources
- Runs server-side, called by the AI model
- Provides:
  - Data queries (provider info, session state, etc.)
  - Transaction builders (constructs unsigned transactions)
  - Oracle data (pricing, insurance calculations)

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                        ChatGPT                               │
│  ┌────────────────┐              ┌────────────────┐         │
│  │  AI Model      │──────────────▶│  UI Widget     │         │
│  │  (LAM logic)   │  Instructions │  (React)       │         │
│  └────────┬───────┘              └────────────────┘         │
│           │                                                  │
│           │ MCP Tool Calls                                   │
│           ▼                                                  │
└───────────┼──────────────────────────────────────────────────┘
            │
            │ HTTPS
            ▼
┌───────────────────────────────────────────────────────────┐
│                    MCP Server                              │
│  ┌────────────────┐        ┌────────────────┐             │
│  │  Tools         │        │  Resources      │             │
│  │  • find_hosts  │        │  • provider_db  │             │
│  │  • open_sess   │        │  • price_feeds  │             │
│  │  • build_tx    │        │  • session_log  │             │
│  └────────┬───────┘        └────────────────┘             │
│           │                                                 │
│           │ RPC Calls                                       │
└───────────┼─────────────────────────────────────────────────┘
            │
            ▼
┌──────────────────────────────────────────────────────────┐
│                  Solana Network                           │
│  ┌──────────────────┐  ┌──────────────────┐              │
│  │  mode_registry   │  │ collateral_vault │              │
│  └──────────────────┘  └──────────────────┘              │
│  ┌──────────────────┐  ┌──────────────────┐              │
│  │  session_escrow  │  │ staking_rewards  │              │
│  └──────────────────┘  └──────────────────┘              │
└──────────────────────────────────────────────────────────┘
```

## Building an MCP Server

### Setup

```bash
npm install @modelcontextprotocol/sdk
```

### Basic MCP Server Structure

```typescript
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

// Initialize MCP server
const server = new Server(
  {
    name: "origin-os-protocol",
    version: "1.0.0",
  },
  {
    capabilities: {
      tools: {},
      resources: {},
    },
  }
);

// Define tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [
      {
        name: "find_providers",
        description: "Find hosting providers by location and capacity",
        inputSchema: {
          type: "object",
          properties: {
            location: { type: "string", description: "Geographic location" },
            minCapacity: { type: "number", description: "Minimum storage in GB" },
          },
          required: ["location"],
        },
      },
      {
        name: "calculate_insurance",
        description: "Calculate insurance coverage and collateral for a session",
        inputSchema: {
          type: "object",
          properties: {
            maxSpend: { type: "number", description: "Max spend in USDC" },
            modeId: { type: "number", description: "Collateral mode (0=USDC, 1=wSOL, 2=WBTC)" },
          },
          required: ["maxSpend", "modeId"],
        },
      },
      {
        name: "build_open_session_tx",
        description: "Build an unsigned transaction to open a session",
        inputSchema: {
          type: "object",
          properties: {
            provider: { type: "string", description: "Provider public key" },
            modeId: { type: "number", description: "Collateral mode" },
            maxSpend: { type: "number", description: "Max spend in tokens" },
            userPubkey: { type: "string", description: "User wallet public key" },
          },
          required: ["provider", "modeId", "maxSpend", "userPubkey"],
        },
      },
    ],
  };
});

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  switch (name) {
    case "find_providers":
      return await handleFindProviders(args);
    
    case "calculate_insurance":
      return await handleCalculateInsurance(args);
    
    case "build_open_session_tx":
      return await handleBuildOpenSessionTx(args);
    
    default:
      throw new Error(`Unknown tool: ${name}`);
  }
});

// Start server
const transport = new StdioServerTransport();
await server.connect(transport);
```

### Implementing Tools

#### Tool: Find Providers

```typescript
import { Connection, PublicKey } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";

async function handleFindProviders(args: any) {
  const { location, minCapacity } = args;
  
  // Connect to Solana
  const connection = new Connection("https://api.devnet.solana.com");
  const program = anchor.workspace.CollateralVault;
  
  // Fetch all provider positions
  const positions = await program.account.providerPosition.all();
  
  // Filter and rank providers
  const providers = positions
    .filter((p) => p.account.totalAmount >= minCapacity)
    .map((p) => ({
      pubkey: p.publicKey.toString(),
      capacity: p.account.totalAmount.toNumber(),
      reserved: p.account.reservedAmount.toNumber(),
      available: p.account.totalAmount.sub(p.account.reservedAmount).toNumber(),
    }))
    .sort((a, b) => b.available - a.available);
  
  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(providers, null, 2),
      },
    ],
  };
}
```

#### Tool: Calculate Insurance

```typescript
async function handleCalculateInsurance(args: any) {
  const { maxSpend, modeId } = args;
  
  // Fetch mode config
  const program = anchor.workspace.ModeRegistry;
  const [registryPda] = await PublicKey.findProgramAddress(
    [Buffer.from("registry")],
    program.programId
  );
  const [modePda] = await PublicKey.findProgramAddress(
    [Buffer.from("mode"), registryPda.toBuffer(), new anchor.BN(modeId).toArrayLike(Buffer, "le", 2)],
    program.programId
  );
  const mode = await program.account.mode.fetch(modePda);
  
  // Insurance formula
  const a = mode.a.toNumber() / 10000; // Convert basis points
  const b = mode.b.toNumber();
  const pMin = mode.pMin.toNumber();
  const pCap = mode.pCap.toNumber();
  
  let coverage = a * maxSpend + b * 100; // Assuming 100 chunks
  coverage = Math.max(pMin, Math.min(pCap, coverage));
  
  const reserveRequired = Math.ceil(coverage * mode.crBps / 10000);
  
  return {
    content: [
      {
        type: "text",
        text: JSON.stringify({
          coverageAmount: coverage,
          collateralRequired: reserveRequired,
          mode: modeId,
          formula: `clamp(${pMin}, ${pCap}, ${a} * ${maxSpend} + ${b} * 100)`,
        }, null, 2),
      },
    ],
  };
}
```

#### Tool: Build Open Session Transaction

```typescript
async function handleBuildOpenSessionTx(args: any) {
  const { provider, modeId, maxSpend, userPubkey } = args;
  
  const program = anchor.workspace.SessionEscrow;
  const user = new PublicKey(userPubkey);
  const providerKey = new PublicKey(provider);
  
  // Generate a nonce for this session (in practice, fetch from user's account)
  const nonce = Date.now(); // Or use an incrementing counter from on-chain state
  
  // Derive PDAs
  const [sessionPda] = await PublicKey.findProgramAddress(
    [Buffer.from("sess"), user.toBuffer(), new anchor.BN(nonce).toArrayLike(Buffer, "le", 8)],
    program.programId
  );
  
  // Build transaction
  const tx = await program.methods
    .openSession({
      provider: providerKey,
      modeId,
      maxSpend: new anchor.BN(maxSpend),
      nonce: new anchor.BN(nonce),
      // ... other params
    })
    .accounts({
      session: sessionPda,
      user,
      // ... other accounts
    })
    .transaction();
  
  // Serialize for frontend
  const serialized = tx.serialize({ requireAllSignatures: false }).toString("base64");
  
  return {
    content: [
      {
        type: "text",
        text: JSON.stringify({
          transaction: serialized,
          sessionPda: sessionPda.toString(),
        }),
      },
    ],
  };
}
```

## Building the UI Widget

### Setup

```bash
npx create-react-app origin-widget
cd origin-widget
npm install @solana/web3.js @solana/wallet-adapter-react @solana/wallet-adapter-wallets
```

### Basic Widget Structure

```tsx
import React, { useState, useEffect } from 'react';
import { Connection, PublicKey, Transaction } from '@solana/web3.js';
import { useWallet } from '@solana/wallet-adapter-react';

function OriginWidget() {
  const { publicKey, signTransaction } = useWallet();
  const [providers, setProviders] = useState([]);
  const [selectedProvider, setSelectedProvider] = useState(null);
  
  // Called by LAM with provider data
  useEffect(() => {
    window.addEventListener('message', (event) => {
      if (event.data.type === 'PROVIDERS_LOADED') {
        setProviders(event.data.providers);
      } else if (event.data.type === 'TRANSACTION_READY') {
        handleTransaction(event.data.transaction);
      }
    });
  }, []);
  
  async function handleTransaction(txBase64: string) {
    if (!publicKey || !signTransaction) {
      alert('Please connect your wallet');
      return;
    }
    
    // Deserialize transaction
    const txBuffer = Buffer.from(txBase64, 'base64');
    const tx = Transaction.from(txBuffer);
    
    // Request signature
    const signed = await signTransaction(tx);
    
    // Send to network
    const connection = new Connection('https://api.devnet.solana.com');
    const signature = await connection.sendRawTransaction(signed.serialize());
    
    // Wait for confirmation
    await connection.confirmTransaction(signature);
    
    // Notify LAM of success
    window.parent.postMessage({
      type: 'TRANSACTION_CONFIRMED',
      signature,
    }, '*');
  }
  
  return (
    <div className="origin-widget">
      <h2>Origin OS Protocol</h2>
      
      {providers.length > 0 && (
        <div className="provider-list">
          <h3>Available Providers</h3>
          {providers.map((p) => (
            <div key={p.pubkey} className="provider-card">
              <div>Capacity: {p.available} GB</div>
              <div>Reserved: {p.reserved} GB</div>
              <button onClick={() => setSelectedProvider(p)}>
                Select
              </button>
            </div>
          ))}
        </div>
      )}
      
      {selectedProvider && (
        <div className="session-form">
          <h3>Open Session</h3>
          <p>Provider: {selectedProvider.pubkey.slice(0, 8)}...</p>
          {/* Form inputs for max spend, etc. */}
        </div>
      )}
    </div>
  );
}

export default OriginWidget;
```

## LAM Prompt Engineering

The LAM's behavior is guided by the system prompt you provide. Here's an example:

```
You are an AI assistant for the Origin OS Protocol, a trustless encrypted hosting platform on Solana.

Your role is to help users:
1. Find hosting providers based on their needs (location, capacity, price)
2. Open sessions with providers (calculate insurance, build transactions)
3. Monitor active sessions
4. File claims when providers fail to deliver service

Available tools:
- find_providers(location, minCapacity): Search for providers
- calculate_insurance(maxSpend, modeId): Estimate costs
- build_open_session_tx(...): Create transaction to open session
- get_session_status(sessionId): Check session state
- build_claim_tx(sessionId, claimType): File a claim

Guidelines:
- Always explain insurance costs and collateral requirements clearly
- Warn users about slashing risks for providers
- Use USDC mode (modeId=0) by default unless user prefers another mint
- When building transactions, always call the widget to request wallet signature
- If a session has stalled, guide user through the claim process

Remember: All funds movement requires explicit wallet signatures. Never assume user consent.
```

## User Flows

### Flow 1: User Opens a Session

1. **User**: *"I need 500 GB of encrypted storage"*
2. **LAM**: Calls `find_providers(minCapacity=500)`
3. **LAM**: Displays provider options in widget
4. **User**: Selects a provider
5. **LAM**: Calls `calculate_insurance(maxSpend=100, modeId=0)`
6. **LAM**: Explains: *"This session will cost up to 100 USDC, with 110 USDC insurance coverage. The provider will lock 165 USDC collateral."*
7. **User**: *"Okay, open the session"*
8. **LAM**: Calls `build_open_session_tx(...)`
9. **Widget**: Prompts user to sign transaction
10. **User**: Signs via wallet
11. **Widget**: Sends transaction, waits for confirmation
12. **LAM**: *"Session opened! Session ID: abc123..."*

### Flow 2: Provider Stakes Collateral

1. **Provider**: *"I want to stake my position NFT to earn rewards"*
2. **LAM**: Calls `get_position_details(providerPubkey)`
3. **LAM**: Shows: *"You have 1000 USDC deposited (500 reserved, 500 free). Expected rewards: 10 $ORIGIN/day"*
4. **Provider**: *"Stake it"*
5. **LAM**: Calls `build_stake_position_tx(...)`
6. **Widget**: Prompts signature
7. **Provider**: Signs
8. **LAM**: *"Staking successful! You're now earning rewards."*

### Flow 3: User Files a Claim

1. **User**: *"My session abc123 stopped working, the provider isn't responding"*
2. **LAM**: Calls `get_session_status(abc123)`
3. **LAM**: Checks `last_activity_slot` and `current_slot`
4. **LAM**: *"The provider hasn't redeemed a permit in 2 hours. You can file a stall claim."*
5. **User**: *"File the claim"*
6. **LAM**: Calls `build_claim_tx(abc123, "stall")`
7. **Widget**: Prompts signature
8. **User**: Signs
9. **LAM**: *"Claim filed. The provider's collateral will be slashed and 110 USDC will be sent to you as insurance."*

## Testing Your App

### Local Testing

1. **Start MCP server**:
   ```bash
   node dist/server.js
   ```

2. **Test tools with MCP Inspector**:
   ```bash
   npx @modelcontextprotocol/inspector node dist/server.js
   ```

3. **Run widget locally**:
   ```bash
   npm start
   ```

### Integration Testing

Use the ChatGPT Apps development environment:
1. Register your app in the ChatGPT developer portal
2. Point to your MCP server endpoint (ngrok for local testing)
3. Upload your widget build
4. Test in ChatGPT interface

## Best Practices

### Security

1. **Never trust client input**: Validate all transaction parameters server-side
2. **Use read-only RPC for queries**: Don't rely on client-provided account data
3. **Verify signatures**: Ensure transactions are actually signed by user's wallet
4. **Rate limit**: Prevent abuse of MCP tools

### UX

1. **Explain costs clearly**: Users should understand escrow, insurance, collateral
2. **Show transaction previews**: Let users see what they're signing
3. **Provide status updates**: Keep users informed during long operations
4. **Handle errors gracefully**: Network issues, insufficient funds, etc.

### Performance

1. **Cache provider data**: Don't query on-chain for every provider search
2. **Use WebSockets**: Subscribe to session updates for real-time status
3. **Lazy load**: Only fetch data when needed
4. **Prefetch common data**: Mode configs, mint info, etc.

## Deployment

### MCP Server

Deploy your MCP server to a hosting service:
- Vercel (Node.js serverless)
- AWS Lambda
- Google Cloud Run
- Dedicated server (for WebSocket support)

Ensure `/mcp` endpoint is publicly accessible over HTTPS.

### UI Widget

Build and host your widget:
```bash
npm run build
# Upload dist/ to CDN or static hosting
```

Update your ChatGPT app configuration with the widget URL.

## Resources

- [Model Context Protocol Specification](https://modelcontextprotocol.io/)
- [ChatGPT Apps Documentation](https://platform.openai.com/docs/guides/apps)
- [Solana Web3.js Documentation](https://solana-labs.github.io/solana-web3.js/)
- [Anchor Framework Guide](https://www.anchor-lang.com/)

## Example Apps

Reference implementations:
- **Origin OS Client** (coming soon): Full-featured client app with LAM integration
- **Provider Dashboard** (coming soon): Provider management interface
- **Explorer** (coming soon): Session and collateral explorer

---

For questions or to showcase your app, join the Origin OS community Discord.
