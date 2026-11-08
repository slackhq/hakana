$i = false;
$b = rand(0, 1) === 1;
foreach (vec[$b] as $n) {
    $i = $n;
    if ($i) {
        continue;
    }
}
if ($i) {}