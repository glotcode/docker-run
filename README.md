# docker-run

## Overview
docker-run provides a http api for running code inside transient docker containers.
For every run request a new container is started and deleted.
The payload is passed to the container by attaching to it and writing it to stdin. The result is read from stdout.
The communication with the docker daemon happens via it's api over the unix socket.
This is used to run code on [glot.io](https://glot.io).
See the [overview](https://github.com/prasmussen/glot) on how everything is connected.


## Performance
The following numbers were obtained using [glot-images](https://github.com/glotcode/glot-images)
on a 5$ linode vm running 'Hello World' with [httpstat](https://github.com/reorx/httpstat)
multiple times locally on the same host and reading the numbers manually.
Not scientific numbers, but it will give an indication of the overhead involved.

| Language         | Min          | Max          |
|:-----------------|:-------------|:-------------|
| Python           | 250 ms       | 350 ms       |
| C                | 330 ms       | 430 ms       |
| Haskell          | 500 ms       | 700 ms       |
| Java             | 2000 ms      | 2200 ms      |

#### With [gVisor](https://gvisor.dev/) (optional)

| Language         | Min          | Max          |
|:-----------------|:-------------|:-------------|
| Python           | 450 ms       | 570 ms       |
| C                | 1300 ms      | 1500 ms      |
| Haskell          | 1760 ms      | 2060 ms      |
| Java             | 4570 ms      | 4800 ms      |


## Installation instructions
See [docs/install](docs/install)


## Environment variables

#### Required

| Variable name                          | Type                          | Description                                                                  |
|:---------------------------------------|:------------------------------|:-----------------------------------------------------------------------------|
| SERVER_LISTEN_ADDR                     | &lt;ipv4 address&gt;          | Listen ip                                                                    |
| SERVER_LISTEN_PORT                     | 1-65535                       | Listen port                                                                  |
| SERVER_WORKER_THREADS                  | &lt;integer&gt;               | How many simultaneous requests that should be processed                      |
| API_ACCESS_TOKEN                       | &lt;string&gt;                | Access token is required in the request to run code                          |
| DOCKER_UNIX_SOCKET_PATH                | &lt;filepath&gt;              | Path to docker unix socket                                                   |
| DOCKER_UNIX_SOCKET_READ_TIMEOUT        | &lt;seconds&gt;               | Read timeout                                                                 |
| DOCKER_UNIX_SOCKET_WRITE_TIMEOUT       | &lt;seconds&gt;               | Write timeout                                                                |
| DOCKER_CONTAINER_HOSTNAME              | &lt;string&gt;                | Hostname inside container                                                    |
| DOCKER_CONTAINER_USER                  | &lt;string&gt;                | User that will execute the command inside the container                      |
| DOCKER_CONTAINER_MEMORY                | &lt;bytes&gt;                 | Max memory the container is allowed to use                                   |
| DOCKER_CONTAINER_NETWORK_DISABLED      | &lt;bool&gt;                  | Enable or disable network access from the container                          |
| DOCKER_CONTAINER_ULIMIT_NOFILE_SOFT    | &lt;integer&gt;               | Soft limit for the number of files that can be opened by the container       |
| DOCKER_CONTAINER_ULIMIT_NOFILE_HARD    | &lt;integer&gt;               | Hard limit for the number of files that can be opened by the container       |
| DOCKER_CONTAINER_ULIMIT_NPROC_SOFT     | &lt;integer&gt;               | Soft limit for the number of processes that can be started by the container  |
| DOCKER_CONTAINER_ULIMIT_NPROC_HARD     | &lt;integer&gt;               | Hard limit for the number of processes that can be started by the container  |
| DOCKER_CONTAINER_CAP_DROP              | &lt;space separated list&gt;  | List of capabilies to drop                                                   |
| RUN_MAX_EXECUTION_TIME                 | &lt;seconds&gt;               | Maximum number of seconds a container is allowed to run                      |
| RUN_MAX_OUTPUT_SIZE                    | &lt;bytes&gt;                 | Maximum number of bytes allowed from the output of a run                     |


## Docker images
When a run request is posted to docker-run it will create a new temporary container.
The container is required to listen for a json payload on stdin and must write the
run result to stdout as a json object containing the properties: stdout, stderr and error.
The docker images used by [glot.io](https://glot.io) can be found [here](https://github.com/glotcode/glot-images).


## Api
| Action                       | Method | Route      | Requires token |
|:-----------------------------|:-------|:-----------|:---------------|
| Get service info             | GET    | /          | No             |
| Get docker info              | GET    | /version   | Yes            |
| [Run code](api_docs/run.md)  | POST   | /run       | Yes            |
