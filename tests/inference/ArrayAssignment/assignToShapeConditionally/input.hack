function foo(): ?shape('include_permissions' => string) {
    $args = dict[];

    $bool_str = vec['true', 'false'];
    foreach ($bool_str as $bool) {
        $args['include_permissions'] = $bool;

        return $args;
    }

    return null;
}