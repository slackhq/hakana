$foo = false;

try {
    if (rand(0, 1) !== 0) {
        throw new \Exception("bad");
    }

    $foo = rand(0, 1) !== 0;

    if ($foo) {}
} catch (Exception $e) {}

if ($foo) {}
