# Running migrations

Hakana can help you perform Hack migrations that require more information than you get from [HHAST](https://github.com/hhvm/hhast).

First, you need to [write a plugin](authoring_plugins.md) for the migration and compile it into Hakana. Once you know what migration you want to call you can run the following command in your terminal:

```
hakana migrate --migration=<migration_name> --symbols=<path_to_symbol_list>
```

Running migrations is reasonably straightforward — you specify your migration and the path of a file containing newline-separated symbol names, and Hakana does the rest for you.

## HHAST vs Hakana

HHAST helps us find and fix issues that humans could identify at a glance — things like a function name with incorrect capitalisation, or a block of code that disobeys Slack-specific formatting rules.

The abstract syntax tree that Hakana uses doesn’t care how your code is formatted.

HHAST, on the other hand, uses a format-preserving abstract syntax tree. That means it produces different representations of code that is functionally identical:

```php
if ($foo)
    bar();
```
        
is treated differently to

```php
if ($foo) {
    bar();
}
```
    
This is great, because it allows us to come up with linter rules to prohibit the first pattern.

But HHAST doesn’t have any idea how your code behaves, so we can’t use it to (for example) prohibit calling a particular instance method in a set of files.

If we want a more nuanced analysis we can turn to Hakana.
