$a = dict[];

if (rand(0, 1) !== 0) {
    $a["a"] = 5;
}

if (HH\Lib\C\count($a) !== 0) {}
if (HH\Lib\C\count($a) === 0) {}
