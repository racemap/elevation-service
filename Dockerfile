FROM geodata/gdal:2.1.3

RUN pip install osr numpy bottle gunicorn

COPY lru.py /app/lru.py
COPY server.py /app/server.py
COPY elevation.py /app/elevation.py

WORKDIR /app

CMD ["python", "server.py"]