$a = null;

do {
    if (rand(0, 1)) {
        break;
    }

    $a = vec['hello'];
} while (rand(0, 1));

echo $a[0];