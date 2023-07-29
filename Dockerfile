FROM rust:buster

RUN apt update --fix-missing
RUN apt install -y \
    python3 \
    python3-pip

RUN pip3 install coverage

COPY . /app
WORKDIR /app

RUN cargo build --release

EXPOSE 8000

CMD ["cargo", "run", "--release"]
