$a = vec["b", "c", "d"];
$a = Vec\take($a, 2);
while (!C\is_empty($a)) {
    $letter = C\lastx($a);
    $a = Vec\take($a, C\count($a) - 1);
    if (C\is_empty($a)) {}
}