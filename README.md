# equailizer
## Overview
equailizer is a tool for batching, splitting, and reconciling "proxy transactions" (i.e. small loans) between two individuals. I made this so that my wife and I can keep better track of our spending habits individually, since we split so many purchases/expenses. equailizer is built on top of the [Lunch Money API](https://lunchmoney.dev).

Why is it called "equailizer"? It is a balance equalizing tool, and quails are very cute.

## Setup

See `eq-config.json.example` for config structure. Create a file named `eq-config.json` with the fields specified.

## Inspiration and caveats

equailizer is written in Rust. This is the first program I have ever made in Rust, and I just wanted to get a good sense of the language and see what it's all about.

One thing I have learned is that Rust was not the right tool for this job. This program is essentially a basic web API client with some business logic on top, made to be used about twice a week. The more I learned what Rust was about, the more I realized that, for a task like this, I do not need the kinds of memory safety and performance that it is built to facilitate. I'm sure there are lots of inefficient things and odd constructs in this code for a Rust program.

However, it was fun to learn about! Years of programming defensively on high-performance devices has gotten me away from thinking about pointer manipulation beyond "do I need to copy this to be safe", but Rust's ownership system is really quite a robust way of doing efficient operations without introducing subtle access/mutation bugs.
