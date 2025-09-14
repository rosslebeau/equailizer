# equailizer
## Overview
`equailizer` is a tool for batching, splitting, and reconciling "proxy transactions" (i.e. small loans) between two individuals. I made this so that my wife and I can keep better track of our spending habits individually, since we split so many purchases/expenses. `equailizer` is built on top of the [Lunch Money API](https://lunchmoney.dev).

Why is it called "`equailizer`"? It is a portmanteau of equalizer and quail.

## Setup

See `eq-config.json.example` for config structure. Create a file named `eq-config.json` with the fields specified.

## Inspiration and caveats

`equailizer` is written in Rust. This is the first program I have ever made in Rust, and I just wanted to get a good sense of the language and see what it's all about.

Rust was not the tool I expected for this job. This program is essentially a basic web API client with some business logic on top, made to be used about twice a week. I thought I'd go in and write some quick hacky stuff, but Rust pretty actively forces you to be intentional with every interface. This is its key to safety and performance, but it's also a drawback in terms of development effort. This project could have been done faster in some scripting language. It would be a "worse" piece of software, but would it matter? Probably not, in this case. But there is a satisfaction in writing precise software.
