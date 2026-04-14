<<file:__EnableUnstableFeatures('like_type_hints')>>
<<file:__EnableUnstableFeatures('upcast_expression')>>

final class Foo {}

function trust_me(vec<shape('type' => string, ...)> $blocks, Foo $foo): ~?dict<arraykey, mixed> {
    foreach ($blocks as $block) {
        $sections = Shapes::idx($block, 'elements') upcast ~?vec<dict<arraykey, mixed>>;
        foreach ($sections ?? vec[] as $section) {
            $leaf_elements = ($section['elements'] ?? null) upcast ~?vec<dict<arraykey, mixed>>;
            foreach ($leaf_elements ?? vec[] as $el) {
                if (($el['type'] ?? null) === $foo) {
                    return $el;
                }
            }
        }
    }
    return null;
}
