function parts(string $input): ((string, string), (string, string)) {
    return tuple(tuple('safe', $input), tuple($input, 'safe'));
}

list(list($a, $b), list($c, $d)) = parts((string)HH\global_get('_GET')['q']);
echo $b;
