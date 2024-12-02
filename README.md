README.md

# SPEAR: Distributed AI Agent Platform

SPEAR is an advanced AI Agent platform designed to support multiple runtime environments. It provides flexibility and scalability for running AI agent workloads in various configurations. SPEAR is currently in development, with ongoing features and improvements.

## Features

- **Runtime Support**:
  - Process
  - Docker Container
  - *Future Support*: WebAssembly and Kubernetes (K8s)
  
- **Operating Modes**:
  - **Local Mode**: Run a single AI agent workload on a local machine.
  - **Cluster Mode**: Designed to support AI agent workloads across multiple clusters. *(Not yet implemented)*

## Build Instructions

To build SPEAR and its related components, run the following command:

```bash
make
```

This command will:
 - Compile all required binaries.
 - Build Docker images for the related AI Agent workloads.

## Usage

To run SPEAR in local mode, use the following command:

```bash
export OPENAI_API_KEY=<YOUR_OPENAI_API_KEY>
bin/worker exec -i 6
```

This command will:
 - Start the SPEAR worker process in local mode.
 - Run the AI agent workload with an ID of 6. (pyconversation-local)

Also, you need to set the environment variable `OPENAI_API_KEY` to your OpenAI API key. In the future, we will support other LLM providers.

## Dependencies
  PortAudio is required for the audio processing component. To install PortAudio on MacOS, use the following command:
  
  ```bash
  brew install portaudio
  ```

## Development Status

 Supported Runtimes:
 - Process
 - Docker Container
 - Planned Runtimes:
 - WebAssembly
 - Kubernetes
 
 Supported Platforms:
 - Currently developed and tested only on macOS.
 - Other platforms have not yet been tested or supported.

## Future Plans

 - Implementation of cluster mode to enable distributed AI agent workloads across multiple clusters.
 - Expansion of runtime support to include WebAssembly and Kubernetes.
 - Cross-platform support for environments beyond macOS.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request to discuss new features, bug fixes, or enhancements.

## License

This project is licensed under the Apache License 2.0.
