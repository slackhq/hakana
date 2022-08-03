$s = rand(0, 1) ? "a" : "b";
if (rand(0, 1)) {
    $s = "c";
}

if ($s === "a" || $s === "b") {
    if ($s === "a") {}
}