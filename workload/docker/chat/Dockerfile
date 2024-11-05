FROM ubuntu:24.04

# update the package lists
RUN apt-get update -y
RUN apt-get install -y netcat-openbsd net-tools

WORKDIR /

# COPY ./scripts/start.sh /start
COPY ./bin/start /start

CMD ["sleep", "infinity"]
