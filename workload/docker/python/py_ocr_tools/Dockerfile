FROM python:3.9.20

# update the package lists
RUN apt-get update -y
RUN apt-get install -y netcat-openbsd net-tools libgl1-mesa-glx
RUN pip3 install paddlepaddle paddleocr

WORKDIR /

# COPY ./scripts/start.sh /start
COPY ./workload/docker/python/py_ocr_tools/src/start.py /start

COPY ./sdk/python/dist/spear*.whl /tmp/
RUN pip install /tmp/spear*.whl
RUN rm /tmp/spear*.whl

RUN python3 -c 'import paddleocr; ocr = paddleocr.PaddleOCR(use_angle_cls=True, lang="en")' || echo

CMD ["sleep", "infinity"]
