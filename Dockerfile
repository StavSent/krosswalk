FROM rust:latest AS build

RUN USER=root cargo new --bin krosswalk
WORKDIR /krosswalk

# copy over your manifests
COPY Cargo.toml Cargo.lock .

# this build step will cache your dependencies
RUN cargo build --release
RUN rm src/*.rs

# copy your source tree
COPY ./src ./src

# build for release
RUN rm ./target/release/deps/krosswalk*
RUN cargo build --release

FROM debian:latest

# install docker tools
USER root
RUN apt-get update \
	&& apt-get install -y sudo \
	&& sudo apt-get update \
	&& sudo apt-get install -y curl gnupg ca-certificates \
	&& sudo install -m 0755 -d /etc/apt/keyrings \
	&& curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
	&& sudo chmod a+r /etc/apt/keyrings/docker.gpg \
	&& echo \
		"deb [arch="$(dpkg --print-architecture)" signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian \
		"$(. /etc/os-release && echo "$VERSION_CODENAME")" stable" | \
		sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

RUN sudo apt-get update \
	&& sudo apt-get -y install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin \
	&& rm -rf /var/lib/apt/lists/*

ENV DOCKER_CONTAINER_ID ""

COPY --from=build /krosswalk/target/release/krosswalk .
CMD ["./krosswalk"]
