FROM raven:latest as raven

RUN apt-get update && apt-get install -y curl jq

COPY .actor/ .actor

ENTRYPOINT [".actor/actor.sh"]
