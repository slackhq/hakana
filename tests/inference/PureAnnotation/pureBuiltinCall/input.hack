final class Date
{
    public static function timeZone(string $tzString)[] : DateTimeZone
    {
        return new \DateTimeZone($tzString);
    }
}