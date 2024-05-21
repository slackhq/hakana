interface EmptyInterface {}

final class StringCastable implements EmptyInterface
{
    public function __toString()
    {
        return 'I am castable';
    }
}

function factory(): EmptyInterface
{
    return new StringCastable();
}

$object = factory();
if (method_exists($object, '__toString')) {
    $a = (string) $object;
    echo $a;
}

if (is_callable(vec[$object, '__toString'])) {
    $a = (string) $object;
    echo $a;
}