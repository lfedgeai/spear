# SPEAR: Distributed AI Agent Platform

SPEAR is an advanced AI Agent platform designed to support multiple runtime environments. It provides flexibility and scalability for running AI agent workloads in various configurations. SPEAR is currently in development, with ongoing features and improvements.

## Features
<table border="1" cellspacing="0" cellpadding="10" style=" width: 100%;">
  <tr>
    <td style="width: 30%; font-weight: bold;">Features</td>
    <td style="width: 35%; font-weight: bold;">Support</td>
    <td style="width: 35%; font-weight: bold;">Status</td>
  </tr>
  <tr>
    <td rowspan="4" style="font-weight: bold;">Runtime Support</td>
    <td>Process</td>
    <td>✅ Supported</td>
  </tr>
  <tr>
    <td>Docker Container</td>
    <td>✅ Supported</td>
  </tr>
  <tr>
    <td>WebAssembly</td>
    <td>⏳ Work in Progress</td>
  </tr>
  <tr>
    <td>Kubernetes</td>
    <td>⏳ Work in Progress</td>
  </tr>
  <tr>
    <td rowspan="2" style="font-weight: bold;">Operating Modes</td>
    <td>Local Mode</td>
    <td>✅ Supported</td>
  </tr>
  <tr>
    <td>Cluster Mode</td>
    <td>⏳ Work in Progress</td>
  </tr>
  <tr>
    <td style="font-weight: bold;">Deployment</td>
    <td>Auto Deployment</td>
    <td>⏳ Work in Progress</td>
  </tr>
  <tr>
    <td rowspan="3" style="font-weight: bold;">Agent Service</td>
    <td>Planning</td>
    <td rowspan="3">⏳ Work in Progress</td>
  </tr>
  <tr>
    <td>Memory</td>

  </tr>
  <tr>
    <td>Tools</td>
  </tr>
</table>


- **Runtime Support**:
  - Process
  - Docker Container
  - *Future Support*: WebAssembly and Kubernetes (K8s)
  
- **Operating Modes**:
  - **Local Mode**: Run a single AI agent workload on a local machine.
  - **Cluster Mode**: Designed to support AI agent workloads across multiple clusters. *(Not yet implemented)*
  
- **Deployment**:
  - **Auto deployment**: Auto Generate configuration files based on programming code.

- **Agent Service**:
  - **Planning**: Offer some agent planning technology enhancing agent ability.
  - **Memory**: Provide some memory services to manage the knowledge of the agent.
  - **Tools**: Provide the user with some built-in tools, and allow the user to customize their own tools.

## Linux OS installation 

### Dependencies
  SPEAR relies on some other third-party software dependency packages. To install this packages on Linux, use the following command:
  
  ```bash
  python -m pip install --upgrade pip
  pip install build
  apt install portaudio19-dev libx11-dev libxtst-dev
  curl -fsSL https://get.docker.com -o get-docker.sh
  sh get-docker.sh
  ```

### Build Instructions

To build SPEAR and its related components, run the following command:

```bash
make
```

This command will:
 - Compile all required binaries.
 - Build Docker images for the related AI Agent workloads.

### Usage

To run SPEAR in local mode, use the following command:

```bash
export OPENAI_API_KEY=<YOUR_OPENAI_API_KEY>
export HUGGINGFACEHUB_API_TOKEN=<YOUR_HUGGINGFACEHUB_API_TOKEN>
export HOST_IP=<YOUR_LOCAL_HOST_IP>
bin/worker exec -n pyconversation
```

This command will:
 - Start the SPEAR worker process in local mode.
 - Run the AI agent workload with an ID of 6. (pyconversation-local)

Also, you need to set the environment variable `OPENAI_API_KEY` to your OpenAI API key. In the future, we will support other LLM providers.



## Mac OS installation 

### Dependencies
  PortAudio is required for the audio processing component. To install PortAudio on MacOS, use the following command:
  
  ```bash
  brew install portaudio
  ```
### Build Instructions

To build SPEAR and its related components, run the following command:

```bash
make
```

This command will:
 - Compile all required binaries.
 - Build Docker images for the related AI Agent workloads.

### Usage

To run SPEAR in local mode, use the following command:

```bash
export OPENAI_API_KEY=<YOUR_OPENAI_API_KEY>
bin/worker exec -n pyconversation
```

This command will:
 - Start the SPEAR worker process in local mode.
 - Run the AI agent workload with an ID of 6. (pyconversation-local)

Also, you need to set the environment variable `OPENAI_API_KEY` to your OpenAI API key. In the future, we will support other LLM providers.


### Development Status

 Supported Runtimes:
 - Process
 - Docker Container
  
 Planned Runtimes:
 - WebAssembly
 - Kubernetes

## Future Plans

 - Implementation of cluster mode to enable distributed AI agent workloads across multiple clusters.
 - Expansion of runtime support to include WebAssembly and Kubernetes.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request to discuss new features, bug fixes, or enhancements.

## License

This project is licensed under the Apache License 2.0.