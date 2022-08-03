
function foo(int $value): void {
    echo $value;
}

foo($_GET["foo"]);

function bar(): int {
    return $_GET["foo"];
}

echo bar();