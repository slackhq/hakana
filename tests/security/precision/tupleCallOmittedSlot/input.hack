function parts(string $input): (string, string, string) {
    return tuple($input, 'safe', $input);
}

list(, $safe) = parts((string)HH\global_get('_GET')['q']);
echo $safe;
