FROM rust:buster

RUN apt update --fix-missing && apt install -y \
    python3 \
    python3-pip \
    libblas3 \ 
    liblapack3 \ 
    liblapack-dev \ 
    libblas-dev \
    gfortran \
    libffi-dev \
    libssl-dev


RUN pip3 install coverage
# install some popular python packages
RUN pip3 install numpy
RUN pip3 install pandas
RUN pip3 install torch

# install nodejs with nvm
ENV NVM_DIR /root/.nvm
ENV NODE_VERSION 20.12.2
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash \
    && . $NVM_DIR/nvm.sh \
    && nvm install $NODE_VERSION \
    && nvm alias default $NODE_VERSION \
    && nvm use default \
    && ln -s $NVM_DIR/versions/node/v$NODE_VERSION/bin/node /usr/bin/node \
    && ln -s $NVM_DIR/versions/node/v$NODE_VERSION/bin/npm /usr/bin/npm \
    && ln -s $NVM_DIR/versions/node/v$NODE_VERSION/bin/npx /usr/bin/npx

# install typescript
RUN npm install -g typescript

# add tsc to path
ENV PATH="/root/.nvm/versions/node/v20.12.2/bin:${PATH}"

COPY . /app
WORKDIR /app

RUN cargo build --release

EXPOSE 8000

CMD ["cargo", "run", "--release"]
