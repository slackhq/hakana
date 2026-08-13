$foo = null;

if (rand(0, 1) !== 0) {
    $foo = "bar";
}

$bar = "bar";

if ($foo === "bar") {}
if ($foo !== "bar") {}
