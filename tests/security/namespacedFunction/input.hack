namespace ns;

function identity(string $s) : string {
    return $s;
}

echo identity($_GET['userinput']);