$b = rand(0, 1) !== 0;

do {
    if (!$b) {
       $b = rand(0, 1) === 0;
    }
} while (!$b);

if ($b) {}