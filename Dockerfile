FROM node:10

COPY . /app
WORKDIR /app
RUN yarn install

ENV TILE_SET_CACHE 128
ENV TILE_SET_PATH /app/data
ENV MAX_POST_SIZE 700kb

EXPOSE 3000

HEALTHCHECK CMD curl --fail http://localhost:3000/status || exit 1

CMD ["yarn", "run", "start"]
