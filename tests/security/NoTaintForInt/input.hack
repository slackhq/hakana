
function foo(int $value): void {
    echo $value;
}

foo(HH\global_get('_GET')["foo"]);

function bar(): int {
    return HH\global_get('_GET')["foo"];
}

echo bar();