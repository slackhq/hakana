$x = 1;
foreach (vec[1, 2, 3] as $i) {
    if ($i > 2) {
        $x = $i;
    }
}
if ($x > 2) {
    echo "found";
}