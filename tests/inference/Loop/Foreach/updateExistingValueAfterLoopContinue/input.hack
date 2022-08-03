$i = false;
$b = (bool) rand(0, 1);
foreach (vec[$b] as $n) {
    $i = $n;
    if ($i) {
        continue;
    }
}
if ($i) {}