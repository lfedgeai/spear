FROM python:3.9.20

# update the package lists
RUN apt-get update -y
RUN apt-get install -y netcat-openbsd net-tools

RUN pip install --no-cache-dir --upgrade pip
RUN pip install --no-cache-dir py-spy

WORKDIR /

# COPY ./scripts/start.sh /start
COPY ./workload/docker/python/pyconversation-local/src/start.py /start

COPY ./sdk/python/dist/spear*.whl /tmp/
RUN pip install --no-cache-dir /tmp/spear*.whl
RUN rm /tmp/spear*.whl

CMD ["sleep", "infinity"]
