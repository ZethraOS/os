# ZethraOS Personal Assistant (Jarvis) — System Architecture v2.0
### System Design, Security Framework, Local Inference Pipeline, and Regulatory Compliance

**Status**: Draft for Technical Review  
**Audience**: Core OS Engineering Team, External AI Lab Partners (Anthropic, OpenAI, DeepSeek, xAI), MAANG Technical Leadership  
**Last Revised**: 2026-05

---

> [!IMPORTANT]
> **CTO Review Note — v1.0 to v2.0 Gap Analysis**
>
> Version 1.0 of this document established a solid foundation — a low-latency local inference pipeline, a basic tool dispatcher, and a two-class HITL security model. However, after rigorous review against production-grade AI system design principles seen at scale across Google, Meta, Apple, Amazon and Microsoft, **12 critical architectural gaps** were identified. These gaps would have resulted in a system that is insecure, non-scalable, legally non-compliant, and incapable of delivering a Jarvis-grade user experience. This document is the corrected, complete specification.

---

## Table of Contents

1. [Gap Analysis — v1.0 Critical Findings](#1-gap-analysis--v10-critical-findings)
2. [Design Philosophy](#2-design-philosophy)
3. [Full System Topology](#3-full-system-topology)
4. [Identity, Personalization, and User Memory Layer](#4-identity-personalization-and-user-memory-layer)
5. [Multi-Device Mesh Architecture](#5-multi-device-mesh-architecture)
6. [Voice, Vision, and Multimodal Input Pipeline](#6-voice-vision-and-multimodal-input-pipeline)
7. [Core Orchestration Engine (`zethra-assistantd`)](#7-core-orchestration-engine-zethra-assistantd)
8. [Multi-Agent Architecture](#8-multi-agent-architecture)
9. [Tool Execution, IPC, and Schema Registry](#9-tool-execution-ipc-and-schema-registry)
10. [Formal Threat Model and Security Architecture](#10-formal-threat-model-and-security-architecture)
11. [Observability, Telemetry, and Auditability](#11-observability-telemetry-and-auditability)
12. [Performance, Power, and Thermal Management](#12-performance-power-and-thermal-management)
13. [Regulatory Compliance (GDPR, CCPA, EU AI Act)](#13-regulatory-compliance-gdpr-ccpa-eu-ai-act)
14. [Phased Delivery Roadmap](#14-phased-delivery-roadmap)
15. [Open Technical Challenges](#15-open-technical-challenges)

---

## 1. Gap Analysis — v1.0 Critical Findings

| # | Gap | Severity | Impact If Not Fixed |
| :---: | :--- | :--- | :--- |
| **G-01** | **No Identity or User Memory model.** The assistant has no persistent memory of who the user is. Every conversation starts from zero. | 🔴 Critical | Impossible to deliver a personal Jarvis. Jarvis knows Tony Stark's preferences, schedule, biometrics, and emotional patterns. |
| **G-02** | **No Multi-Device Topology.** The document assumes a single screen. ZethraOS must run on phones, TVs, tablets, AR/VR, and laptops simultaneously, each as a node of the same assistant mesh. | 🔴 Critical | The assistant cannot hand off a conversation from your phone to your TV. Context is siloed per device. |
| **G-03** | **No Formal Threat Model.** Listing "prompt injection" as the only threat is dangerously incomplete. No adversary models, no trust boundaries, no formal attack surface analysis. | 🔴 Critical | The system would be approved by developers and fail immediately in a penetration test or regulatory audit. |
| **G-04** | **Binary HITL Gate is too coarse.** `toggle_wifi` as "Low-Impact" is wrong. Turning off Wi-Fi during navigation could kill routing and strand a user. All tool impact classifications need context-sensitive risk scoring. | 🔴 Critical | Silently executing dangerous actions or constantly annoying users with confirmations for trivial ones. Both cases destroy trust. |
| **G-05** | **No Explainability or Reasoning Transparency Layer.** The system produces actions with no mechanism to answer *"Why did you do that?"*. | 🔴 Critical | Fatal for regulatory compliance (EU AI Act Article 13), user trust, and debugging failures. |
| **G-06** | **No Multi-Agent Architecture.** A single 7B model cannot simultaneously handle voice transcription, deep calendar reasoning, code generation, and API calls. The orchestrator must spawn specialized sub-agents. | 🟠 High | The system will be slow, inaccurate, and hit context-window limits on complex tasks within months of launch. |
| **G-07** | **No Schema Registry or Tool Versioning.** The IPC JSON examples are static. Production systems require a versioned schema registry for tools, preventing breaking changes from cascading across the OS. | 🟠 High | A future OS update changes a socket protocol field; the assistant silently starts failing all tool calls in the field with zero diagnostics. |
| **G-08** | **Thermal Model is Battery-Only.** The model scaling table reacts to battery level but has no real-time SoC temperature sensor integration, no prediction, and no hysteresis — it will oscillate between models. | 🟠 High | Thermal oscillation (repeatedly loading and unloading model weights) wastes more power than running a slightly larger model steadily. |
| **G-09** | **No Observability or Audit Trail.** There is zero mention of logging, tracing, or monitoring for the assistant's decisions or tool calls. | 🟠 High | Impossible to debug, impossible to audit for compliance, and impossible to detect when the assistant is being adversarially manipulated. |
| **G-10** | **No Regulatory Compliance Section.** The assistant processes voice, faces, location, and health sensor data continuously. GDPR, CCPA, and EU AI Act obligations are not addressed. | 🟠 High | Illegal to ship to Europe and California. A single fine under GDPR Article 83 could exceed €20 million or 4% of global annual revenue. |
| **G-11** | **No TTS Architecture.** The voice pipeline ends at STT → LLM. There is no Text-to-Speech specification, emotional tone modelling, or voice persona design for the response output. | 🟡 Medium | The assistant cannot speak. It is a text chatbot with a wake word, not Jarvis. |
| **G-12** | **No Phased Delivery Roadmap.** This is a 5-year build. Without a phased roadmap with milestones, the engineering team will attempt to ship everything at once and ship nothing on time. | 🟡 Medium | Project management failure and loss of investor/engineering confidence. |

---

## 2. Design Philosophy

A Jarvis-grade AI assistant embedded into an operating system is fundamentally different from a chatbot.  
It must satisfy the **CAPRIC** principles:

| Principle | Requirement |
| :--- | :--- |
| **C**ontinuity | Persistent memory of the user's identity, preferences, history, and goals across every conversation and every device. |
| **A**gency | Capable of multi-step, autonomous task execution without hand-holding for every sub-step. |
| **P**rivacy | All sensitive inference must execute on-device. No private data (voice, face, health, location) leaves the device without explicit, informed, revocable consent. |
| **R**eliability | Must degrade gracefully: if NPU is overloaded, fall back to smaller model; if offline, use cached context. |
| **I**nterpretability | Every action taken must be auditable. The user can always ask "why did you do that?" and receive a clear explanation. |
| **C**ompliance | By default, every behaviour must be compliant with GDPR, CCPA, and the EU AI Act from day one. |

---

## 3. Full System Topology

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                          ZethraOS Device Mesh (Multi-Screen Node)                       │
│                                                                                         │
│  ┌─────────────────────────────────────────────────────────────────────────────────┐    │
│  │                       INPUT LAYER (Sensor Fusion)                               │    │
│  │  [Mic HAL] [Camera HAL] [Sensor HAL] [Compositor] [Biometric HAL] [App Events]  │    │
│  └──────────────────────────────────┬──────────────────────────────────────────────┘    │
│                                     │ raw signals                                       │
│  ┌──────────────────────────────────▼──────────────────────────────────────────────┐    │
│  │                SIGNAL PROCESSING LAYER (User Space, DSP + NPU)                  │    │
│  │  [Wake Word Engine] [VAD] [Whisper STT] [VLM Visual Encoder] [Sensor Fusion]    │    │
│  └──────────────────────────────────┬──────────────────────────────────────────────┘    │
│                                     │ structured modality tokens                        │
│  ┌──────────────────────────────────▼──────────────────────────────────────────────┐    │
│  │              ORCHESTRATION LAYER (zethra-assistantd core)                       │    │
│  │                                                                                 │    │
│  │  ┌────────────────┐  ┌─────────────────┐  ┌─────────────────────────────────┐   │    │
│  │  │ Context Manager│  │ Memory Store    │  │ Multi-Agent Orchestrator        │   │    │
│  │  │ (KV-cache +    │  │ (Vector DB +    │  │ (Planner Agent + Specialist     │   │    │
│  │  │  Summarizer)   │  │  Episodic Store)│  │  Agents: Code, Calendar, etc.)  │   │    │
│  │  └────────────────┘  └─────────────────┘  └─────────────────────────────────┘   │    │
│  │                                                                                 │    │
│  │  ┌────────────────┐  ┌─────────────────┐  ┌─────────────────────────────────┐   │    │
│  │  │  Trust Engine  │  │ Explainability  │  │  Observation & Audit Logger     │   │    │
│  │  │ (Context-aware │  │ Trace Buffer    │  │  (Immutable ring buffer)        │   │    │
│  │  │  Risk Scoring) │  │                 │  │                                 │   │    │
│  │  └────────────────┘  └─────────────────┘  └─────────────────────────────────┘   │    │
│  └──────────────────────────────────┬──────────────────────────────────────────────┘    │
│                                     │ tool calls + responses                            │
│  ┌──────────────────────────────────▼──────────────────────────────────────────────┐    │
│  │               TOOL EXECUTION LAYER (Guardrail + Schema Registry)                │    │
│  │  [HITL Gate] [Rate Limiter] [Schema Validator v1/v2/...] [Dry-Run Sandbox]      │    │
│  └──────────────────────────────────┬──────────────────────────────────────────────┘    │
│                                     │ JSON IPC over Unix sockets                        │
│  ┌──────────────────────────────────▼──────────────────────────────────────────────┐    │
│  │                    SYSTEM SERVICES LAYER (zethrad-managed)                      │    │
│  │  [Telephony] [Network] [Compositor] [Biometric] [OTA] [Storage] [App Sandbox]   │    │
│  └─────────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                         │
│  ←──────────── Encrypted Device Mesh Sync (mDNS + Noise Protocol) ──────────────────►   │
│         Phone Node      ←→      TV Node      ←→    Laptop Node    ←→   AR/VR Node       │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Identity, Personalization, and User Memory Layer

> [!IMPORTANT]
> This is the single most important section that was entirely missing from v1.0. A truly personal assistant is only possible with a persistent, structured user memory. Without it, every conversation is a first meeting.

### 4.1 Three-Tier Memory Architecture

The memory system is modelled after human cognition: immediate working memory, episodic recall, and long-term semantic knowledge.

| Memory Tier | Type | Storage | Content | Example |
| :--- | :--- | :--- | :--- | :--- |
| **Working Memory** | In-context (KV-cache) | NPU SRAM / fast DRAM | Current conversation, active task state | "User said they are feeling tired" |
| **Episodic Memory** | Vector embeddings | Local encrypted SQLite + FAISS index on `/data/zethra/assistant/memory/` | Significant past events, explicit user preferences, past decisions | "User prefers morning meetings. Last time reminded to call mum on her birthday." |
| **Semantic / World Memory** | Static model weights + RAG | On-device model, device-local document chunks | General knowledge, user's documents, calendar, contacts, health data | "User's doctor is Dr. Priya. User is lactose intolerant." |

### 4.2 Memory Privacy Controls

*   All memory files under `/data/zethra/assistant/memory/` are encrypted using the device's hardware-backed key (Keystore/Trusty TEE equivalent).
*   Users can inspect, edit, and permanently delete any individual memory record via the **Memory Settings** UI.
*   Memory synchronization across the device mesh is performed only after explicit opt-in and is end-to-end encrypted using `Noise_XX` protocol.

### 4.3 Retrieval-Augmented Generation (RAG) at Inference Time

At each orchestration turn, relevant memories are retrieved from the episodic store using a vector similarity search and injected into the LLM context as a structured memory block:

```rust
// Pseudocode — Context injection at inference
fn build_context(query: &str, working_mem: &KvCache) -> Prompt {
    let episodic = memory_store.retrieve(query, top_k = 5);
    let device_state = device_monitor.snapshot();
    Prompt::builder()
        .system(SYSTEM_INSTRUCTIONS)
        .injected_memory(episodic)
        .device_state(device_state)
        .conversation(working_mem.last_n_turns(10))
        .user_query(query)
        .build()
}
```

---

## 5. Multi-Device Mesh Architecture

A user's ZethraOS devices (phone, TV, tablet, AR/VR, laptop) form a **local-first encrypted mesh** where the assistant's context, active task, and attention are co-ordinated in real time.

### 5.1 Attention Handoff Protocol

The mesh uses a concept of **Active Attention Node** — only one device is in full active inference mode at any time. Others are in low-power passive listening mode.

```
[User is watching TV]
  TV Node     = Active Attention (7B model, full display context, audio output)
  Phone Node  = Passive (1B wake-word only, screen off)
  Laptop Node = Passive (screen locked, 1B background agent only)

[User picks up phone]
  Mesh detects proximity (BT RSSI + accelerometer)
  Phone Node broadcasts "intent to take attention"
  TV Node responds with encrypted context handoff package:
     { conversation_state, active_tasks, last_turn_summary }
  Phone Node becomes Active Attention
  TV Node drops to Passive within 500ms
```

### 5.2 Mesh Transport Layer

*   **Discovery**: mDNS over the local network (`_zethraassist._tcp`).
*   **Transport**: QUIC (UDP) with `Noise_XX` handshake for E2E encryption between nodes.
*   **Packet Format**: Protocol Buffers (`proto3`) for zero-copy, schema-versioned payloads.
*   **Fallback**: If not on the same network, context handoff uses a user-controlled relay server (self-hosted or ZethraCloud opt-in).

---

## 6. Voice, Vision, and Multimodal Input Pipeline

### 6.1 Always-On Wake Word Engine
*   **Engine**: Custom-trained ONNX micro-model (`< 100KB`), specific to the keyword "Jarvis" (or user-configured).
*   **Execution Target**: Ultra-low-power DSP core running at `< 1 mA`. Main SoC remains in deep sleep.
*   **False-Positive Rate Target**: `< 1 per 24 hours` of continuous ambient audio.
*   **False-Negative Rate Target**: `< 5%` in high-noise environments (factory floor, crowded street).

### 6.2 Voice Activity Detection & Speech-to-Text

```
[Mic PCM Stream @ 16kHz, mono]
    ↓
[Silero VAD — ONNX Runtime, DSP] — (Endpoint Detection, < 50ms latency)
    ↓
[Audio segment: confirmed speech utterance]
    ↓
[Whisper.cpp Small.en / Multilingual FP16 on NPU]
    ↓
[Raw transcript text]
    ↓
[Language Identification] → [Route to appropriate multilingual sub-model if non-English]
    ↓
[Text tokens → Orchestrator]
```

**Latency budget**: `VAD ≈ 50ms` + `STT ≈ 150ms` + `LLM first token ≈ 200ms` + `TTS ≈ 150ms` = **`< 600ms` total TTFA (Time To First Audio)**.

### 6.3 Text-to-Speech (TTS) Response Output

> [!NOTE]
> This was entirely missing from v1.0. The assistant must be able to speak, not just text-output.

*   **Engine**: `StyleTTS2` or `VITS2`, quantized to INT8, running on NPU.
*   **Voice Persona**: Configurable. Default is a neutral, professional voice. User can train a custom voice persona from 10 seconds of enrolled audio.
*   **Streaming TTS**: Audio synthesis begins on the **first generated sentence** while the LLM continues generating subsequent tokens. This pipelines TTS and LLM generation, eliminating a full LLM completion wait.
*   **Emotional Tone**: TTS expressiveness adjusts based on message sentiment derived from the LLM's output annotations (e.g., `[urgent]`, `[calm]`, `[informational]`).

### 6.4 Vision Stack (Screen Context & Camera)

*   **Screen Context**: A secured `zethra_screen_ai` Wayland extension delivers a compositor-side frame capture. Secure-flagged windows are rendered black **in the compositor**, before the frame buffer is even written to shared memory. No sensitive pixels ever reach `zethra-assistantd`.
*   **Camera / AR Context**: For AR/VR devices, a `VisionEncoder` (`SigLIP` / `CLIP` ONNX model, INT8) converts camera frames to dense image embeddings, injected into the multimodal prompt.
*   **Face Recognition / Presence Detection**: For user identification and attention detection, a biometric HAL (`/run/zethra/biometric.sock`) provides presence signals without raw frame exposure to the LLM stack.

---

## 7. Core Orchestration Engine (`zethra-assistantd`)

### 7.1 Structured Inference Loop

```
1. INPUT        → Fuse voice tokens, screen embeddings, sensor state, RAG memory
2. PLAN         → Planner agent generates a chain-of-thought step plan
3. DISPATCH     → Specialist sub-agents execute steps (see §8)
4. TRUST CHECK  → Tool Dispatcher runs context-aware risk scoring (see §10)
5. EXECUTE      → Approved tools run via IPC; results injected back into context
6. RESPOND      → LLM synthesises final response; TTS renders audio
7. RECORD       → Audit logger records the full action trace; memory updated
```

### 7.2 Context and KV-Cache Management

The context window of a 7B model (typically `8,192` to `32,768` tokens) is managed via a **priority eviction policy**:

| Priority | Context Block | Eviction Policy |
| :--- | :--- | :--- |
| **P0 — Pinned** | System prompt, safety rules, user identity facts | Never evicted |
| **P1 — Hot** | Current turn's RAG results, active task state | Evicted only when space critical |
| **P2 — Warm** | Last 10 conversation turns | Evicted after summarization |
| **P3 — Cold** | Historical turns (> 10) | Summarized and evicted; summary promoted to episodic memory |

A background `context-summarizer` thread runs asynchronously to compress P3 blocks before forced eviction so no information is permanently lost.

---

## 8. Multi-Agent Architecture

> [!IMPORTANT]
> A single generalist model cannot do everything well. This is a fundamental limitation of current LLM architectures at 3B-7B scale on-device. ZethraOS uses a Planner + Specialist model hierarchy.

```
User Query: "Jarvis, book me a flight to Bangalore for next Thursday,
             remind my wife, and make sure my laptop is charged by 6am"
                                  │
                                  ▼
                        [Planner Agent — 7B]
                     "I need 3 specialist agents"
                        /         |          \
                       /          |           \
                      ▼           ▼            ▼
            [Travel Agent]  [Comms Agent]  [Device Agent]
         (Query flights API) (Draft + send  (Set alarm +
          + confirm booking)  SMS to wife)   charging schedule)
```

### 8.1 Agent Registry

Each specialist agent is a stateless Rust function registered in the `AgentRegistry`, capable of being loaded on-demand and hot-reloaded by OTA updates:

```rust
// Agent registration (agent/registry.rs)
pub struct AgentDefinition {
    pub id: &'static str,           // e.g. "comms_agent_v2"
    pub version: semver::Version,
    pub capabilities: &'static [Capability],
    pub model_hint: ModelSize,      // Tiny / Small / Medium
    pub max_parallel_instances: u8,
}
```

### 8.2 Inter-Agent Communication

*   Agents communicate via a typed Rust channel (`tokio::mpsc`) within `zethra-assistantd`.
*   Cross-device agent invocations (e.g., "agent on TV, what is the user watching?") use the Mesh Transport Layer (§5.2).
*   Agents are stateless by design. All state is managed by the Context Manager and Memory Store.

---

## 9. Tool Execution, IPC, and Schema Registry

> [!NOTE]
> Tool schemas must be versioned. A future OS update must never silently break the assistant.

### 9.1 Schema Registry

All tool definitions are stored in a central schema registry at `/etc/zethra/assistant/tools/`:

```json
// /etc/zethra/assistant/tools/telephony.schema.json
{
  "tool_id": "telephony.send_message",
  "version": "2.1.0",
  "description": "Send an SMS or RCS message to a contact.",
  "impact_class": "HIGH_IMPACT",
  "socket": "/run/zethra/telephony.sock",
  "parameters": {
    "recipient": { "type": "string", "format": "e164_phone", "required": true },
    "body":      { "type": "string", "max_length": 1600, "required": true }
  },
  "confirmation_template": "Send message to {{recipient}}: \"{{body}}\""
}
```

The schema registry is loaded at daemon startup and hot-reloaded on OTA updates. Older schema versions are retained for backward compatibility for a minimum of two major OS releases.

### 9.2 Context-Sensitive Risk Scoring (Replaces Binary HITL)

The v1.0 binary High/Low Impact classification is replaced by a **dynamic, multi-dimensional risk score** computed at execution time:

```
RiskScore = BaseImpact × ContextMultiplier × HistoryMultiplier × TimingMultiplier

Where:
  BaseImpact        = Static tool classification (0.1 to 1.0)
  ContextMultiplier = e.g., "toggle Wi-Fi" while navigating = ×5.0; at home idle = ×0.5
  HistoryMultiplier = Tool never used before = ×2.0; used 100 times = ×0.5
  TimingMultiplier  = 3am action on device locked for 6 hours = ×10.0; normal business hours = ×1.0
```

| RiskScore Range | Action |
| :--- | :--- |
| `0.0 – 0.30` | Execute silently, log to audit trail. |
| `0.31 – 0.70` | Show non-blocking status notification: *"Jarvis: Toggling Wi-Fi off."* (Cancellable within 3s). |
| `0.71 – 1.00` | Full HITL block: require biometric or PIN confirmation before execution. |

---

## 10. Formal Threat Model and Security Architecture

### 10.1 Adversary Models

| Adversary | Goal | Capability | Mitigation |
| :--- | :--- | :--- | :--- |
| **Remote attacker via network** | Execute privileged tool calls | Send crafted network packets or emails | SELinux + BPF LSM; assistant never listens on network sockets. |
| **Malicious app on device** | Hijack assistant via fake IPC | Register a rogue `/run/zethra/fake.sock` | Tool Dispatcher only connects to sockets listed in the Schema Registry. Sockets are owned by `zethra-system` user (UID 1000), writable only by the owning service. |
| **Prompt injection via untrusted text** | Force assistant to call High-Impact tools | Craft malicious text in email, SMS, webpage | Context Isolation (§10.3): untrusted content is processed in a sandboxed context with all tool calls disabled. |
| **Physical attacker (device stolen)** | Access memory / conversation history | Full physical device access | All memory files encrypted with TEE-backed key, requiring biometric unlock to decrypt. Memory is zeroed on 10 failed unlock attempts. |
| **Supply chain attack on model weights** | Inject adversarial behaviour into model | Tamper with the model file on disk | Model weight files are verified via `Ed25519` signature against the OTA signing key before loading. A tampered model file is rejected with a fallback to the previously verified version. |
| **Social engineering via Jarvis** | Trick assistant into revealing secrets | Ask "What did my wife text me last night?" | Intent classification layer: information disclosure queries about third parties require explicit per-query consent. |

### 10.2 Trust Boundaries

```
╔══════════════════════════════════════════════════════════════════╗
║  TRUSTED ZONE (runs in restricted SELinux domain)               ║
║  zethra-assistantd, Schema Registry, Memory Store               ║
╠══════════════════════════════════════════════════════════════════╣
║  SEMI-TRUSTED ZONE (IPC only, no direct memory access)          ║
║  zethra-telephonyd, zethra-networkd, zethra-compositor          ║
╠══════════════════════════════════════════════════════════════════╣
║  UNTRUSTED ZONE (sandboxed, no tool execution rights)           ║
║  App Sandboxes, External Web Content, Email Bodies, SMS Bodies  ║
╚══════════════════════════════════════════════════════════════════╝
```

### 10.3 Context Isolation for Untrusted Input Processing

When the assistant reads untrusted content (email body, SMS, webpage, document), it runs a **two-phase pipeline**:

*   **Phase 1 — Isolated Reader (no tools)**: A sandboxed inference pass reads and summarizes the content. Tool call generation is **disabled at the decoding layer** by suppressing the tool-call special token from the model's output logits — not just via a system prompt instruction, which can be bypassed.
*   **Phase 2 — Trusted Orchestrator**: The sanitized summary (not the raw untrusted text) is injected into the main orchestrator context. Only now can tool calls be considered.

### 10.4 Explainability and Audit Trail

> [!IMPORTANT]
> This is required by the EU AI Act (Article 13, Transparency) and essential for user trust.

Every assistant action is recorded to an immutable append-only ring buffer at `/data/zethra/assistant/audit/`:

```json
{
  "trace_id": "uuid-v4",
  "timestamp": "2026-05-22T11:38:40Z",
  "user_query_hash": "sha256:...",     // hashed, not raw, for privacy
  "plan": ["step 1: retrieve contacts", "step 2: draft message", "step 3: await confirmation"],
  "tools_called": [
    { "tool": "contacts.lookup", "args_hash": "sha256:...", "result": "success" },
    { "tool": "telephony.send_message", "risk_score": 0.82, "hitl": "biometric_confirmed", "result": "success" }
  ],
  "model_used": "llama-3.2-7b-q4",
  "device_node": "phone-primary"
}
```

The user can review this audit trail in plain English through the **Jarvis Transparency Panel** in System Settings.

---

## 11. Observability, Telemetry, and Auditability

*   **Structured Logging**: All daemon logs output structured JSON to `journald`, tagged with `unit=zethra-assistantd`.
*   **Distributed Tracing**: Each user request generates a `trace_id` (UUID v4) propagated across every sub-agent, tool call, and IPC message, enabling full end-to-end latency profiling.
*   **Privacy-Safe Telemetry** (opt-in only): Aggregate, differentially private statistics (e.g., p50/p95 TTFA latency, tool failure rate by category — **never raw voice, queries, or personal data**) may be reported to ZethraOS engineering for fleet-wide performance monitoring.
*   **On-Device Analytics Dashboard**: A local-only `Jarvis Health` view shows the user the assistant's performance stats, memory usage, tool call history, and last inference model used.

---

## 12. Performance, Power, and Thermal Management

### 12.1 Dynamic Model Scaling with Thermal Hysteresis

The v1.0 model scaled on battery level alone, causing thermal oscillation. The corrected model uses a hysteresis band and SoC temperature as the primary signal:

| Condition | Active Model | Inference Backend | Scale-Up Threshold | Scale-Down Threshold |
| :--- | :--- | :--- | :--- | :--- |
| **Cool** (SoC < 55°C) + Battery > 40% | 7B INT8 | NPU High-Freq | — | SoC > 65°C or Battery < 30% |
| **Warm** (SoC 55–70°C) or Battery 20-40% | 3B INT4 | NPU Low-Freq | SoC < 50°C + Bat > 45% | SoC > 75°C or Battery < 15% |
| **Hot** (SoC > 70°C) or Battery < 20% | 1B INT4 | CPU only (efficiency cores) | SoC < 60°C + Bat > 25% | — |
| **Critical** (SoC > 80°C) or Battery < 5% | Suspended — text-only fallback via cloud | Cloud relay (opt-in) | SoC < 65°C + Bat > 10% | — |

The hysteresis band (separate up/down thresholds) prevents rapid model switching, which wastes memory bandwidth and power loading/unloading model weights from flash storage.

### 12.2 Memory and Zero-Copy Design

*   **KV-Cache pages**: Memory-locked via `mlock()` to prevent NPU tensor cache from being swapped to disk. On low-memory pressure, the cache is partially serialized to encrypted fast storage rather than fully evicted.
*   **Audio Pipeline**: PCM audio from the Mic HAL to the VAD and STT is passed via a lock-free ring buffer in shared memory — zero kernel-space copies in the hot path.
*   **Vision Pipeline**: Frame buffers from the Wayland compositor are passed via `dmabuf` file descriptors — zero-copy GPU/NPU memory sharing between compositor and VLM encoder.

### 12.3 Latency Budget (Target: TTFA < 800ms)

| Pipeline Stage | Target Latency |
| :--- | :--- |
| Wake word detection to SoC wakeup | < 80ms |
| VAD endpoint detection | < 50ms |
| Whisper STT transcription | < 150ms |
| RAG memory retrieval | < 30ms |
| LLM first token generation | < 250ms |
| TTS first audio syllable (streaming) | < 150ms |
| **Total Time To First Audio (TTFA)** | **< 720ms** |

---

## 13. Regulatory Compliance (GDPR, CCPA, EU AI Act)

> [!CAUTION]
> Failure to design for compliance from day one makes retroactive compliance extremely expensive. Apple paid $14.8M in Italy for Siri privacy violations in 2023. Google paid $391M to US states for deceptive location tracking.

| Regulation | Requirement | ZethraOS Implementation |
| :--- | :--- | :--- |
| **GDPR Art. 17** | Right to erasure | "Jarvis, forget everything" triggers a cryptographic key rotation that permanently destroys access to all encrypted memory files. |
| **GDPR Art. 20** | Data portability | All episodic memory is exportable as a standard JSON archive on user request. |
| **GDPR Art. 25** | Privacy by design | All inference runs on-device by default. Cloud fallback is strictly opt-in and requires informed consent at first setup. |
| **CCPA §1798.100** | Right to know what data is collected | Transparent `Jarvis Memory` settings panel shows every stored memory record in plain English. |
| **EU AI Act Art. 13** | Transparency & Explainability | Every action is logged in the audit trail (§10.4). Users can query the audit log in natural language: *"Why did you send that message?"* |
| **EU AI Act Art. 9** | Risk management system | Context-sensitive risk scoring (§9.2) is the implementation of this. All tool classifications must be documented and justified. |
| **COPPA (US)** | Child protection | Device profiles flagged as minor (`age < 13`) disable all outbound communication tools, purchase capabilities, and disable memory persistence by default. |

---

## 14. Phased Delivery Roadmap

| Phase | Timeline | Milestone | Key Deliverables |
| :--- | :--- | :--- | :--- |
| **Phase 1 — Foundation** | Months 1–6 | Local Voice Loop | Wake word, VAD, Whisper STT, basic TTS, single-turn Q&A with no tool calls. Mock tool runner. |
| **Phase 2 — Tool Execution** | Months 7–12 | Jarvis Can Act | Schema Registry, IPC tool dispatcher, Risk Scoring v1, HITL gate, Telephony + Network + Calendar tools. |
| **Phase 3 — Memory & Persona** | Months 13–18 | Jarvis Knows You | Episodic memory (Vector DB), RAG at inference time, user identity model, Transparency Panel, GDPR compliance audit. |
| **Phase 4 — Multi-Agent** | Months 19–24 | Jarvis Can Plan | Planner + Specialist agent hierarchy, multi-step autonomous task execution, Agent Registry. |
| **Phase 5 — Multi-Device Mesh** | Months 25–30 | Jarvis is Everywhere | Attention Handoff protocol, Mesh Transport Layer, context sync across Phone + TV + Tablet + Laptop + AR/VR nodes. |
| **Phase 6 — Multimodal & Ambient** | Months 31–36 | Jarvis Sees & Hears | VLM visual encoder, screen context, camera perception, proactive (unsolicited) suggestions, ambient intelligence. |

---

## 15. Open Technical Challenges

1.  **Model Weight Integrity in the Field**: If a user sideloads a community model (e.g., a custom fine-tuned Llama variant), the Ed25519 signing chain breaks. We need a clear policy: either restrict to signed official models (Apple-style) or implement a user-acknowledged "untrusted model" mode that disables High-Impact tools — similar to Android's Unknown Sources toggle.

2.  **Multi-Language STT Accuracy on Low-Resource Languages**: Whisper Small is excellent on English, Spanish, Mandarin, and Hindi. Its word error rate for languages like Tamil, Swahili, and Bengali is significantly higher. For ZethraOS to serve global markets equitably, we need a language-specific STT fine-tuning and evaluation pipeline.

3.  **Mesh Conflict Resolution**: What happens when a user issues a voice command simultaneously on two devices? (e.g., speaking into a phone while a TV picks up the same voice). The Active Attention protocol must implement a conflict resolution mechanism (RSSI + microphone confidence score based bidding) to deterministically elect one winner node within `< 100ms`.

4.  **Long-Horizon Planning and Memory Coherence**: Multi-day autonomous tasks (e.g., "Plan my trip to Bangalore next week — book flights, hotel, remind me to pack") require the assistant to maintain a persistent task graph across device reboots, model reloads, and battery cycles. This is an unsolved problem in production AI agent systems and requires a dedicated task persistence engine beyond the episodic memory store.

5.  **Hallucination in Tool Parameter Generation**: LLMs will occasionally hallucinate tool parameters (e.g., generating a valid-looking but incorrect phone number). A dry-run validation pass — where tool parameters are constructed and type-checked against the schema before HITL confirmation — is mandatory. Users must see the exact parameters before high-impact execution.
