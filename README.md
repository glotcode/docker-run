# docker-run

## Overview
docker-run provides a http api for running untrusted code inside transient docker containers.
For every run request a new container is started and deleted.
The payload is passed to the container by attaching to it and writing it to stdin. The result is read from stdout.
The communication with the docker daemon happens via it's api over the unix socket.
This is used to run code on [glot.io](https://glot.io).


## Api
| Action                       | Method | Route      | Requires token |
|:-----------------------------|:-------|:-----------|:---------------|
| Get service info             | GET    | /          | No             |
| Get docker info              | GET    | /version   | Yes            |
| [Run code](api_docs/run.md)  | POST   | /run       | Yes            |


## Docker images
When a run request is posted to docker-run it will create a new temporary container.
The container is required to listen for a json payload on stdin and must write the
run result to stdout as a json object containing the properties: stdout, stderr and error.
The docker images used by [glot.io](https://glot.io) can be found [here](https://github.com/glotcode/glot-images).


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


## Security
Docker containers are not as secure as a vm and there has been weaknesses in the past
where people have been able to escape a container in specific scenarios.
The recommended setup is to store any database / user data / secrets on a separate machine then the one that runs docker + docker-run,
so that if anyone is able to escape the container it will limit what they get access to.
That said, glot.io has been running untrusted  code in docker containers since 2015 without any issues.

Depending on your use-case you should also consider to:
* Disable network access using `DOCKER_CONTAINER_NETWORK_DISABLED`
* Drop [capabilities](https://man7.org/linux/man-pages/man7/capabilities.7.html) using `DOCKER_CONTAINER_CAP_DROP`
* Use the [gVisor](https://gvisor.dev/) runtime


## Installation instructions
- [Run docker-run in a docker container](docs/install/docker-ubuntu-20.10.md)
- [Run docker-run with systemd](docs/install/ubuntu-20.10.md)
- [gVisor](docs/install/ubuntu-20.10-gvisor.md)


## FAQ

**Q:** How is fork bombs handled?

**A:** The number of processes a container can create can be set with the `DOCKER_CONTAINER_ULIMIT_NPROC_HARD` variable.

##

**Q:** How is infinite loops handled?

**A:** The container will be killed when the `RUN_MAX_EXECUTION_TIME` value is reached.

##

**Q:** How is large output handled?

**A:** Docker-run will stop reading the output from the container when it has read the number of bytes defined in `RUN_MAX_OUTPUT_SIZE`.

##

**Q:** How is high memory usage handled?

**A:** The max memory for a container can be set with the `DOCKER_CONTAINER_MEMORY` variable.


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


#### Optional

| Variable name                          | Type                          | Description                                                                  |
|:---------------------------------------|:------------------------------|:-----------------------------------------------------------------------------|
| DOCKER_CONTAINER_READONLY_ROOTFS       | &lt;bool&gt;                  | Mount root as read-only (recommended)                                        |
| DOCKER_CONTAINER_TMP_DIR_PATH          | &lt;filepath&gt;              | Will add a writeable tmpfs mount at the given path                           |
| DOCKER_CONTAINER_TMP_DIR_OPTIONS       | &lt;string&gt;                | Mount options for the tmp dir (default: rw,noexec,nosuid,size=65536k)        |
| DOCKER_CONTAINER_WORK_DIR_PATH         | &lt;filepath&gt;              | Will add a writeable tmpfs mount at the given path                           |
| DOCKER_CONTAINER_WORK_DIR_OPTIONS      | &lt;string&gt;                | Mount options for the work dir (default: rw,exec,nosuid,size=131072k)        |
| DEBUG_KEEP_CONTAINER                   | &lt;bool&gt;                  | Don't remove the container after run is completed (for debugging)            |
