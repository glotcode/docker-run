let
  nixpkgs =
    builtins.fetchGit {
      url = "https://github.com/NixOS/nixpkgs";
      ref = "refs/heads/release-23.05";
      rev = "9117c4e9dc117a6cd0319cca40f2349ed333669d";
    };

  pkgs =
    import nixpkgs {};

  dockerRun =
    import ./default.nix { pkgs = pkgs; };
in
pkgs.dockerTools.buildImage {
  name = "glot/docker-run";
  tag = "latest";
  created = "now";

  config = {
    Env = [
      "LANG=C.UTF-8"
      "SERVER_LISTEN_ADDR=0.0.0.0"
      "SERVER_LISTEN_PORT=8088"
      "SERVER_WORKER_THREADS=10"
      "API_ACCESS_TOKEN=some-secret-token"
      "DOCKER_UNIX_SOCKET_PATH=/var/run/docker.sock"
      "DOCKER_UNIX_SOCKET_READ_TIMEOUT=3"
      "DOCKER_UNIX_SOCKET_WRITE_TIMEOUT=3"
      "DOCKER_CONTAINER_HOSTNAME=glot"
      "DOCKER_CONTAINER_USER=glot"
      "DOCKER_CONTAINER_MEMORY=1000000000"
      "DOCKER_CONTAINER_NETWORK_DISABLED=true"
      "DOCKER_CONTAINER_ULIMIT_NOFILE_SOFT=90"
      "DOCKER_CONTAINER_ULIMIT_NOFILE_HARD=100"
      "DOCKER_CONTAINER_ULIMIT_NPROC_SOFT=90"
      "DOCKER_CONTAINER_ULIMIT_NPROC_HARD=100"
      "DOCKER_CONTAINER_CAP_DROP=MKNOD NET_RAW NET_BIND_SERVICE"
      "DOCKER_CONTAINER_READONLY_ROOTFS=true"
      "DOCKER_CONTAINER_TMP_DIR_PATH=/tmp"
      "DOCKER_CONTAINER_TMP_DIR_OPTIONS=rw,noexec,nosuid,size=65536k"
      "DOCKER_CONTAINER_WORK_DIR_PATH=/home/glot"
      "DOCKER_CONTAINER_WORK_DIR_OPTIONS=rw,exec,nosuid,size=131072k"
      "RUN_MAX_EXECUTION_TIME=15"
      "RUN_MAX_OUTPUT_SIZE=100000"
      "RUST_LOG=debug"
    ];

    Cmd = [ "${dockerRun}/bin/docker-run" ];
  };
}
