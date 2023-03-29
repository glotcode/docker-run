# Installation instructions for ubuntu 20.10

#### Install and configure docker

```bash
apt install docker.io

# Disable docker networking (optional)
echo '{
    "ip-forward": false,
    "iptables": false,
    "ipv6": false,
    "ip-masq": false
}' > /etc/docker/daemon.json

systemctl restart docker.service
```

#### Create user for docker-run

```bash
useradd -m glot
usermod -aG docker glot
```

#### Install docker-run binary

```bash
mkdir /home/glot/bin
cd /home/glot/bin
wget https://github.com/glotcode/docker-run/releases/download/v1.4.0/docker-run_linux-x64.tar.gz
tar -zxf docker-run_linux-x64.tar.gz
rm docker-run_linux-x64.tar.gz
chown -R glot:glot /home/glot/bin
```

#### Add and configure systemd service
Most of the configuration from the example file is ok but the `API_ACCESS_TOKEN` should be changed

```bash
curl https://raw.githubusercontent.com/glotcode/docker-run/main/systemd/docker-run.service > /etc/systemd/system/docker-run.service

# Edit docker-run.service in your favorite editor

systemctl enable docker-run.service
systemctl start docker-run.service
```

#### Pull docker images

```bash
docker pull glot/python:latest
docker pull glot/rust:latest
# ...
```

#### Check that everything is working

```bash
# Print docker-run version
curl http://localhost:8088

# Print docker version, etc
curl --header 'X-Access-Token: access-token-from-systemd-service' http://localhost:8088/version

# Run python code
curl --request POST --header 'X-Access-Token: access-token-from-systemd-service' --header 'Content-type: application/json' --data '{"image": "glot/python:latest", "payload": {"language": "python", "files": [{"name": "main.py", "content": "print(42)"}]}}' http://localhost:8088/run
```
