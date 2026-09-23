function parts(string $input): ((string, string), (string, string)) {
    return tuple(tuple('safe', $input), tuple($input, 'safe'));
}

$parts = parts((string)HH\global_get('_GET')['q']);
list(list($a, $b), list($c, $d)) = $parts;
echo $a;
echo $d;
