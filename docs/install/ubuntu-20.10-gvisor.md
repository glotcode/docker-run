# gVisor installation instructions

Installing gVisor is optional, but provides an extra layer of security.

These instructions are based on the [offical gVisor instructions](https://gvisor.dev/docs/user_guide/install/)
and assumes that you already have followed the [docker-run instructions for ubuntu 20.10](ubuntu-20.10.md)

```bash
apt install apt-transport-https ca-certificates curl gnupg-agent software-properties-common
curl -fsSL https://gvisor.dev/archive.key | sudo apt-key add -
add-apt-repository "deb https://storage.googleapis.com/gvisor/releases release main"
apt update
apt install runsc
```

#### Set runsc as the default runtime

Add a `default-runtime` field to `/etc/docker/daemon.json`. The file should look something like this:

```js
{
    ...
    "default-runtime": "runsc",
    "runtimes": {
        "runsc": {
            "path": "/usr/bin/runsc"
        }
    }
}
```

#### Restart the docker daemon

```bash
systemctl restart docker.service
```

The gVisor runtime is now used when running code
