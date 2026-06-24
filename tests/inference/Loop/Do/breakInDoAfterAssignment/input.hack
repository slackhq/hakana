$a = null;

do {
    $a = vec['hello'];
    
    if (rand(0, 1) !== 0) {
        break;
    }
} while (rand(0, 1));

echo $a[0];
