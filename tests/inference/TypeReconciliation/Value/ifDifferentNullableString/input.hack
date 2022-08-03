$foo = null;

if (rand(0, 1)) {
    $foo = "bar";
}

$bar = "bar";

if ($foo === "bar") {}
if ($foo !== "bar") {}