$a = null;

foreach (vec[1, 2, 3] as $i) {
    $a = $i;
    unset($i);
}