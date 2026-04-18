# The Vertex Swarm Challenge 2026
## Coordinating Robots, Drones, AI Agents, and IoT Fleets Beyond the Cloud

Welcome to the **Vertex Swarm Challenge 2026**. This hackathon pushes the boundaries of how we think about coordination. In a world where billions of devices—from warehouse robots to autonomous drones and AI agents—need to work together in real-time, the traditional "cloud-first" model is hitting a wall.

This guide will help you understand the core concepts of Swarm Coordination, the tools at your disposal (Vertex and FoxMQ), and how to choose the right architecture for your project.

---

## 1. Consensus & Swarms: The New Paradigm

### What is Consensus?
At its core, **Consensus** is the process by which a distributed network of participants (nodes) agrees on a single version of the truth, even if some participants are faulty or malicious. In the context of a robotic swarm, consensus means every robot agrees on the exact order of events or movements without needing a central director.

### What is a Swarm?
A **Swarm** is a decentralized collective of autonomous agents that interact locally with one another and their environment. While individual agents may be simple, the collective behavior emerges as complex, coordinated action. Think of a flock of birds or a colony of ants—there is no "leader," yet they move as one.

### Traditional Centralized vs. Swarm Architecture

| Feature | Centralized (Cloud-First) | Swarm (Vertex-Powered) |
| :--- | :--- | :--- |
| **Control** | Single Controller / Cloud Server | Distributed / Peer-to-Peer |
| **Connectivity** | Star Topology (All nodes to center) | Mesh Topology (Node to Node) |
| **Latency** | Dependent on Round-trip to Cloud | Edge-local (<100ms) |
| **Failure** | Single Point of Failure (SPOF) | Byzantine Fault Tolerant (BFT) |
| **Scalability** | Bottlenecked by Central Capacity | Grows with the Network |

### Why Swarms are Superior (Beyond Surface Level)

1. **Byzantine Fault Tolerance (BFT):** In a centralized system, if the controller sends a corrupt command or is hacked, the whole system fails. In a Swarm using Vertex, the network can reach agreement even if 1/3 of the nodes are behaving erratically or maliciously. This is critical for safety-critical hardware like drones.
2. **Fair Ordering:** Traditional central queues (like RabbitMQ) order messages based on when they hit the server. In a high-speed swarm, this isn't "fair" because network jitter can change the perceived order. Swarms use consensus to ensure that the *actual* sequence of events is mathematically preserved across all nodes.
3. **Gasless Edge Processing:** Unlike blockchains which require "gas fees" for every transaction, Vertex allows for high-throughput coordination at the edge. A drone swarm making 1,000 adjustments per second can't wait for a block confirmation or pay a fee for every wing-flap. Swarms process the work where it happens.
4. **Resilience to "Cloud Rip":** A DNS outage or a cloud region failure can ground an entire fleet of centralized robots. A swarm operates as a "Meshnet"—as long as the robots can see each other (via WiFi, LoRa, or 5G), they can keep working.

---

## 2. Introducing Vertex: The Engine

**Vertex** is Tashi’s embedded consensus engine. It is the "brain" that allows individual nodes in a swarm to reach agreement in sub-100ms.

- **Powered by Hashgraph:** Vertex uses a Directed Acyclic Graph (DAG) and "Virtual Voting" to achieve consensus. Unlike traditional blockchains, it never needs to send "vote messages" across the network, making it incredibly fast and efficient.
- **Meshnet Architecture:** When you use Vertex, you aren't just building an app; you're creating a peer-to-peer network where participants are both users and consensus nodes.
- **Private by Default:** Only the final "Proof of Coordination" needs to leave the local swarm. Internal state and intermediate consensus rounds stay within your meshnet.

---

## 3. Introducing FoxMQ: Decentralized Messaging

**FoxMQ** is a decentralized MQTT broker built on top of the Vertex consensus engine. If you are already familiar with IoT development, FoxMQ is your bridge to the swarm.

- **Familiar Protocol:** It speaks MQTT—the industry standard for IoT—meaning your existing robots and sensors can connect to it immediately.
- **Consensus-Backed Queues:** Unlike a standard broker that lives on one server, FoxMQ distributes the queue across the entire swarm. If one broker node goes down, the others don't skip a beat.
- **Fair Broadcast:** FoxMQ ensures that every subscriber receives messages in the exact same order, which is vital for synchronized maneuvers in a drone fleet.

---

## 4. When to Use What?

Choosing your tool depends on how "deep" you want to go into the consensus logic.

### Use **Vertex** if:
- You are building **Custom Logic**: You need specific rules for how robots move or how agents negotiate.
- You need **Maximum Throughput**: You are writing native Rust or C code for high-performance edge devices.
- You want **Native Consensus**: You want to embed agreement directly into your application's state machine.

### Use **FoxMQ** if:
- You have an **Existing Fleet**: Your devices already use MQTT and you want to "decentralize" them without rewriting their firmware.
- You need **Simple Pub/Sub**: Your main goal is sending messages (e.g., sensor data, health pings) across many parties.
- You want **Low Friction**: You need to get up and running quickly with standard MQTT libraries.

---

## 5. Resources

Get started with the following docs and tools:

### Warmup Templates & Kickstarters
- **[FoxMQ Python Warmup](https://github.com/tashigit/warmup-foxmq-python)**: Get started with decentralized MQTT using Python.
- **[Vertex Rust Warmup](https://github.com/tashigit/warmup-vertex-rust)**: A template for building native consensus-powered agents in Rust.
- **[Webots Starter Kit](https://github.com/tashigit/tashi_vertex-webots-starter-kit)**: Orchestrate robotic simulations in Webots using Vertex.
- **[AirSim Vertex Starter Kit](https://github.com/tashigit/airsim-vertex-starter-kit)**: Decentralized drone swarm coordination starter kit using Tashi Vertex P2P consensus and Microsoft AirSim.

### Demos
- **[Nexus Agentic Commerce](https://github.com/tashigit/nexus-agentic-commerce)**: High-Frequency Agentic Commerce built on Tashi Vertex.
- **[Pioneer Swarm Exploration](https://github.com/tashigit/pioneer)**: Comprehensive ROS 2 + Webots stack for coordinating five Pioneer robots that cooperatively explore an arena, share telemetry through FoxMQ, and reach consensus on robot health before accepting detections.

### MCP / Context for agents
- **[Tashi Vertex Rust Context](https://context7.com/tashigit/tashi-vertex-rs)**: Get up-to-date documentation and code context for Tashi Vertex RS optimized for LLMs and AI agents via Context7.
- **[FoxMQ Agent Skill](FoxMQ-SKILL.md)**: A specialized skill definition for FoxMQ. Add this file to your repository's `.agent/skills/` directory (or run `npx ctx7 skills install /tashigit/foxmq foxmq-mqtt-broker`) to give your AI agent deep context on FoxMQ cluster setup, producer-consumer patterns, and consensus-backed MQTT networking.


---

- **[Tashi Documentation](https://docs.tashi.network/)**: The home for all things Vertex and FoxMQ.
- **[Vertex for Rust (vertex-rs)](https://docs.tashi.network/developers/developers/vertex-rs)**: Build native Rust-based swarm agents.
- **[Vertex for C (vertex-c)](https://docs.tashi.network/developers/developers/vertex-c)**: Integrate consensus into C
- **[FoxMQ Resources](https://docs.tashi.network/resources/foxmq)**: Deep dive into the decentralized MQTT broker.
- **[Technical Whitepaper](https://docs.tashi.network/whitepaper/whitepaper)**: For those who want to understand the DAG and virtual voting math.
- **[Tashi Glossary](https://docs.tashi.network/resources/glossary)**: Catch up on the terminology.

---

## 6. What is Tashi?

**Tashi** is the coordination layer for intelligent systems. We provide the infrastructure that allows autonomous machines—robots, drones, and AI agents—to reach agreement in milliseconds without needing to trust a central authority.

Our mission is to **align devices with the interests of their owners**. Traditionally, devices serve the platforms they are connected to; Tashi flips the script by enabling coordination at the edge, where machines operate with verifiable alignment to those they represent.

The Tashi stack consists of three powerful layers:
1.  **Vertex**: The consensus layer (used in this hackathon) providing sub-100ms BFT agreement.
2.  **Lattice**: A global DePIN infrastructure layer for node discovery, validation, and economic rewards.
3.  **Arc**: A settlement bridge that connects private meshnet coordination to public blockchains for payments or public finality.

Tashi is also a co-founding member of the **Intercognitive Foundation**, working alongside pioneers like Auki and Peaq to establish global standards for AI accessibility and the machine economy.

---

**Good luck, Hackers! Build something that moves the world—literally.**