final class HaruDestination {}
final class AClass
{
    public function get(): HaruDestination
    {
        return new HaruDestination();
    }
}