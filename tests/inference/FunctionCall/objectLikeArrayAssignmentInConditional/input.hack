$a = dict[];

if (rand(0, 1)) {
    $a["a"] = 5;
}

if (HH\Lib\C\count($a)) {}
if (!HH\Lib\C\count($a)) {}