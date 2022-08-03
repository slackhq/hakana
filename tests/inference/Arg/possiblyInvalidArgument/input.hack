$foo = vec[
    "a",
    vec["b"],
];

$a = array_map(
    (string $uuid): string ==> {
        return $uuid;
    },
    $foo[rand(0, 1)]
);