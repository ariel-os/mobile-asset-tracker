FROM debian:13

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y pipx && apt-get clean

RUN pipx ensurepath

ENV PORT=4230

COPY ./server.py /app/server.py

WORKDIR /app

EXPOSE 4230

CMD pipx run server.py
