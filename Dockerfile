FROM elleven11/multipl-e-evaluation:latest

RUN apt update --fix-missing && apt install -y \
    cargo \
    libssl-dev


RUN pip3 install coverage
# install some popular python packages
RUN pip3 install pandas
RUN pip3 install torch

COPY . /app
WORKDIR /app

RUN cargo build --release

EXPOSE 8000

ENTRYPOINT ["cargo", "run", "--release"]
