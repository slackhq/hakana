$b = !!rand(0, 1);

do {
    if (!$b) {
       $b = !rand(0, 1);
    }
} while (!$b);

if ($b) {}