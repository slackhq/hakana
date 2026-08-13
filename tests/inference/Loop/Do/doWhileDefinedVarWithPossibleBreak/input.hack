$value = null;
do {
    if (rand(0, 1) !== 0) {
        break;
    }
    $count = rand(0, 1);
    $value = 6;
} while ($count);
