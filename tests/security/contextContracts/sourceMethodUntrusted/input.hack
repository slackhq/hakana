final class Sources {
<<Hakana\SecurityAnalysis\Source('UriRequestHeader'),
  Hakana\SecurityAnalysis\NotSourceWhen('key', 'trusted', 'also-trusted')>>
public static function source(string $key, string $fallback = ''): string { return $fallback; }
}
echo Sources::source('untrusted');
