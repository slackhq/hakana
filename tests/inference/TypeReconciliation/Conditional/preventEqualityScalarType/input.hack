function foo(int $i) : void {
    if ($i === 5) {}
    if (5 === $i) {}
    if ($i === "5") {}
    if ("5" === $i) {}
    if ($i === 5.0) {}
    if (5.0 === $i) {}
    if ($i === 0.0) {}
    if (0.0 === $i) {}
}
function bar(float $i) : void {
    if ($i === 5.0) {}
    if (5.0 === $i) {}
    if ($i === "5") {}
    if ("5" === $i) {}
    if ($i === 5) {}
    if (5 === $i) {}
    if ($i === "0") {}
    if ("0" === $i) {}
    if ($i === 0) {}
    if (0 === $i) {}
}
function bat(string $i) : void {
    if ($i === "5") {}
    if ("5" === $i) {}
    if ($i === 5) {}
    if (5 === $i) {}
    if ($i === 5.0) {}
    if (5.0 === $i) {}
    if ($i === 0) {}
    if (0 === $i) {}
    if ($i === 0.0) {}
    if (0.0 === $i) {}
    $a = new A();
    if ($i === $a) {}
    if ($a === $i) {}
}

final class A {}