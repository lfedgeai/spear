FROM python:3.9.20

# update the package lists
RUN apt-get update -y
RUN apt-get install -y netcat-openbsd net-tools

WORKDIR /

# COPY ./scripts/start.sh /start
COPY ./src/start.py /start

CMD ["sleep", "infinity"]
