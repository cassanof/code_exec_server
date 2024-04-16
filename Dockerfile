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

COPY . /app
WORKDIR /app

RUN cargo build --release

EXPOSE 8000

CMD ["cargo", "run", "--release"]
