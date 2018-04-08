FROM geodata/gdal:2.1.3

RUN pip install osr numpy repoze.lru bottle gunicorn

COPY lru.py /app/lru.py
COPY server.py /app/server.py
COPY tile.py /app/tile.py

WORKDIR /app

CMD ["python", "server.py"]