FROM hackerwasii/raven:latest

RUN apt-get update && apt-get install -y dnsutils whois && \
    rm -rf /var/lib/apt/lists/*

COPY actor.sh /actor.sh
COPY input_schema.json /input_schema.json
COPY dataset_schema.json /dataset_schema.json

RUN chmod +x /actor.sh

ENV CODENAME=raven

ENTRYPOINT ["/actor.sh"]
