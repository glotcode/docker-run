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

# Restart docker daemon
systemctl restart docker.service
```

#### Pull the docker-run image

```bash
docker pull glot/docker-run:latest
```


#### Pull images for the languages you want

```bash
docker pull glot/python:latest
docker pull glot/rust:latest
# ...
```

#### Start the docker-run container

```bash
docker run --detach --restart=always --publish 8088:8088 --volume /var/run/docker.sock:/var/run/docker.sock --env "API_ACCESS_TOKEN=my-token" glot/docker-run:latest
```

#### Check that everything is working

```bash
# Print docker-run version
curl http://localhost:8088

# Print docker version, etc
curl --header 'X-Access-Token: my-token' http://localhost:8088/version

# Run python code
curl --request POST --header 'X-Access-Token: my-token' --header 'Content-type: application/json' --data '{"image": "glot/python:latest", "payload": {"language": "python", "files": [{"name": "main.py", "content": "print(42)"}]}}' http://localhost:8088/run
```
