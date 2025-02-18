$a = vec["b", "c", "d"];
array_pop(inout $a);
while ($a) {
    $letter = array_pop(inout $a);
    if (!$a) {}
}