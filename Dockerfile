FROM node:10

COPY . /app
WORKDIR /app
RUN yarn install

ENV TILE_SET_CACHE 128
ENV TILE_SET_PATH /app/data
ENV MAX_POST_SIZE 700kb

CMD ["yarn", "run", "start"]
