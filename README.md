## Krosswalk

Module (part of a bigger system) that sends a docker container's logs to a kafka queue. It also works as a real-time consumer that based on the content of the incoming messages can scale up or down said container's cpu and memory limits

To run the code all you need is docker and a working kafka broker 

```bash
docker build -t krosswalk .
docker run -d -v /var/run/docker.sock:/var/run/docker.sock --name=krosswalk krosswalk
```

If you want to "forward" device's localhost to this container just add the following to `docker run` command:

```bash
--network=host
```
