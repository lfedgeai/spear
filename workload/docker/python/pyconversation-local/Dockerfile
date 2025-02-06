FROM python:3.12-slim-bookworm
ARG WHL_FILENAME

# update the package lists
RUN apt-get update -y
RUN apt-get install -y netcat-openbsd net-tools

RUN pip install --no-cache-dir --upgrade pip
RUN pip install --no-cache-dir py-spy

WORKDIR /

# COPY ./scripts/start.sh /start
COPY ./workload/docker/python/pyconversation-local/src/start.py /start

COPY ./sdk/python/dist/$WHL_FILENAME /tmp/
RUN pip install --no-cache-dir /tmp/$WHL_FILENAME
RUN rm /tmp/$WHL_FILENAME

CMD ["sleep", "infinity"]
