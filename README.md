# equailizer
## Overview
`equailizer` is a tool for batching, splitting, and reconciling "proxy transactions" (i.e. small loans) between two individuals. I made this so that my wife and I can keep better track of our spending habits individually, since we split so many purchases/expenses. `equailizer` is built on top of the [Lunch Money API](https://lunchmoney.dev).

`equailizer` is organized around the concept of 'batches' of transactions that are handles between the 'creditor' and the 'debtor'. The creditor is the one who has loaned money to the debtor, and the program is really run from the POV of the creditor.

First the creditor creates a batch: The creditor goes in to their budgeting tool and marks which transactions should be added to a batch, with an option to automatically split a transaction evenly and only add half to the batch. The creditor then runs `equailizer create-batch`, which performs these actions and provides the creditor with an action to request reimbursement for the batch all at once.

Then, once the reimbursement is paid, `equailizer reconcile` can reconcile the batch. Reconciliation involves checking the reimbursement transaction, making sure the amount matches the items in the batch, and then updating the debtor's side of the reimbursement transaction in their budgeting tool by splitting it out into transactions that reflect each of the individual transactions the creditor added to the batch. This way, the debtor can mark these individually in their budgeting tool, instead of having one nebulous lump sum paid to the creditor.

## Setup

`equailizer` performs operations in the context of a 'profile'. Each profile is set up as a directory inside a `profiles` directory in the path you run equailizer from. Inside each profile directory, place a `config.json` file, which describes the creditor and debtor details, as well as a JMAP setup section so `equailizer` can automatically send notification emails.

See `config.example.json` for config structure.

## Inspiration and caveats

`equailizer` is written in Rust. This is the first program I have ever made in Rust, and I just wanted to get a good sense of the language and see what it's all about. I do intend for this to be well-written software, but please keep in mind that this is not the work of a Rust expert! There also may be test code, comments, or other unsavory items committed to `main` as this is is an immature code base (although I try to keep this to a minimum).

I wrote this program for myself and my wife, and it is tailored to our use case. I do not anticipate there being any demand or reason to expand it to cover other use cases, or to be configurable beyond what we need. However, I am open to discussion if anyone finds it useful, and it is MIT-licensed so others may learn from it, modify it, or expand on it as they see fit.

## FAQ

#### Why does `equailizer` only support JMAP? Isn't IMAP more popular?

I use Fastmail, and I am hopeful that JMAP (especially the calendar portion) can supersede IMAP/CalDAV one day.

#### Why is it called "`equailizer`"?

It is a portmanteau of equalizer and quail.
