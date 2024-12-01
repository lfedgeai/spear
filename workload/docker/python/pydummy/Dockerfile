FROM python:3.9.20

# update the package lists
RUN apt-get update -y
RUN apt-get install -y netcat-openbsd net-tools

RUN pip install --upgrade pip
RUN pip install py-spy

WORKDIR /

# COPY ./scripts/start.sh /start
COPY ./workload/docker/python/pydummy/src/start.py /start

COPY ./sdk/python/dist/spear*.whl /tmp/
RUN pip install /tmp/spear*.whl
RUN rm /tmp/spear*.whl

CMD ["sleep", "infinity"]
