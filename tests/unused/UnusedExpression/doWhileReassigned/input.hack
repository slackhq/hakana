$a = 5;

do {
    echo $a;
    $a = $a - rand(-3, 3);
} while ($a > 3);