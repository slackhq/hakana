namespace ns;

function identity(string $s) : string {
    return $s;
}

echo identity(\HH\global_get('_GET')['userinput']);