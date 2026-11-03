<?php
/**
 * Plugin Name: Wartungscockpit Maintenance Agent
 * Description: Signierte REST-Endpunkte für Inventar, Plugin- und Core-Updates sowie Health-Checks. Als Mu-Plugin installieren.
 * Version: 0.1.0
 * Author: Wartungscockpit
 * Requires PHP: 7.4
 * Requires at least: 5.6
 *
 * Installation:
 *   1. Datei nach wp-content/mu-plugins/wartungscockpit-agent.php kopieren.
 *   2. In wp-config.php definieren: define('WC_MAINT_SHARED_SECRET', '…');
 *   3. Einen Benutzer mit der Rolle Administrator und ein Application Password anlegen.
 *
 * Sicherheit:
 *   - Jede Anfrage braucht Application Password + HMAC-Signatur + gültigen Zeitstempel.
 *   - Optional IP-Allowlist über den Filter wc_maintenance_allowed_ips.
 *   - Updates nur mit backup_confirmed=true; der Agent erstellt selbst kein Backup.
 */

if (!defined('ABSPATH')) {
    exit;
}

const WC_MAINT_VERSION = '0.1.0';
const WC_MAINT_NAMESPACE = 'maintenance/v1';
const WC_MAINT_LOG_OPTION = 'wc_maintenance_log';
const WC_MAINT_JOB_OPTION = 'wc_maintenance_active_job';

$GLOBALS['wc_maint_job_active'] = false;

add_action('rest_api_init', 'wc_maint_register_routes');
register_shutdown_function('wc_maint_shutdown_handler');

function wc_maint_register_routes(): void
{
    register_rest_route(WC_MAINT_NAMESPACE, '/health', array(
        'methods' => WP_REST_Server::READABLE,
        'callback' => 'wc_maint_health_endpoint',
        'permission_callback' => 'wc_maint_authorize_read',
    ));

    register_rest_route(WC_MAINT_NAMESPACE, '/inventory', array(
        'methods' => WP_REST_Server::READABLE,
        'callback' => 'wc_maint_inventory_endpoint',
        'permission_callback' => 'wc_maint_authorize_read',
        'args' => array(
            'refresh' => array(
                'type' => 'boolean',
                'default' => true,
            ),
        ),
    ));

    register_rest_route(WC_MAINT_NAMESPACE, '/updates/plugins', array(
        'methods' => WP_REST_Server::CREATABLE,
        'callback' => 'wc_maint_update_plugins_endpoint',
        'permission_callback' => 'wc_maint_authorize_plugins',
        'args' => array(
            'plugins' => array(
                'type' => 'array',
                'items' => array('type' => 'string'),
                'default' => array(),
            ),
            'dry_run' => array(
                'type' => 'boolean',
                'default' => false,
            ),
            'backup_confirmed' => array(
                'type' => 'boolean',
                'default' => false,
            ),
            'continue_on_error' => array(
                'type' => 'boolean',
                'default' => false,
            ),
        ),
    ));

    register_rest_route(WC_MAINT_NAMESPACE, '/updates/core', array(
        'methods' => WP_REST_Server::CREATABLE,
        'callback' => 'wc_maint_update_core_endpoint',
        'permission_callback' => 'wc_maint_authorize_core',
        'args' => array(
            'dry_run' => array(
                'type' => 'boolean',
                'default' => false,
            ),
            'backup_confirmed' => array(
                'type' => 'boolean',
                'default' => false,
            ),
        ),
    ));

    register_rest_route(WC_MAINT_NAMESPACE, '/log', array(
        'methods' => WP_REST_Server::READABLE,
        'callback' => 'wc_maint_log_endpoint',
        'permission_callback' => 'wc_maint_authorize_read',
    ));
}

function wc_maint_authorize_read(WP_REST_Request $request)
{
    return wc_maint_authorize($request, 'manage_options');
}

function wc_maint_authorize_plugins(WP_REST_Request $request)
{
    return wc_maint_authorize($request, 'update_plugins');
}

function wc_maint_authorize_core(WP_REST_Request $request)
{
    return wc_maint_authorize($request, 'update_core');
}

/**
 * Prüft HMAC, Zeitstempel, Rate-Limit und Capability.
 *
 * @return true|WP_Error
 */
function wc_maint_authorize(WP_REST_Request $request, string $capability)
{
    $secret = wc_maint_secret();
    if ($secret === '') {
        return new WP_Error(
            'wc_maint_secret_missing',
            'WC_MAINT_SHARED_SECRET ist nicht konfiguriert.',
            array('status' => 503)
        );
    }

    $ip = isset($_SERVER['REMOTE_ADDR']) ? sanitize_text_field(wp_unslash($_SERVER['REMOTE_ADDR'])) : '';
    $allowed_ips = apply_filters('wc_maintenance_allowed_ips', array());
    if (is_array($allowed_ips) && !empty($allowed_ips) && !in_array($ip, $allowed_ips, true)) {
        return new WP_Error('wc_maint_ip_blocked', 'IP-Adresse ist nicht freigegeben.', array('status' => 403));
    }

    if (!wc_maint_rate_limit_ok($ip)) {
        return new WP_Error('wc_maint_rate_limited', 'Zu viele Anfragen. Bitte später erneut versuchen.', array('status' => 429));
    }

    $timestamp = (string) $request->get_header('x-maintenance-timestamp');
    $signature = (string) $request->get_header('x-maintenance-signature');

    if ($timestamp === '' || $signature === '') {
        return new WP_Error('wc_maint_signature_missing', 'HMAC-Header fehlen.', array('status' => 401));
    }

    if (abs(time() - (int) $timestamp) > 300) {
        return new WP_Error('wc_maint_timestamp_invalid', 'Zeitstempel ist älter als fünf Minuten.', array('status' => 401));
    }

    $signature = preg_replace('/^sha256=/i', '', $signature);
    $expected = hash_hmac('sha256', $timestamp . '.' . $request->get_body(), $secret);

    if (!is_string($signature) || !hash_equals($expected, $signature)) {
        return new WP_Error('wc_maint_signature_invalid', 'HMAC-Signatur ist ungültig.', array('status' => 401));
    }

    if (!current_user_can($capability)) {
        return new WP_Error('wc_maint_capability_missing', 'Fehlende Berechtigung: ' . $capability, array('status' => 403));
    }

    return true;
}

function wc_maint_secret(): string
{
    if (defined('WC_MAINT_SHARED_SECRET')) {
        return (string) WC_MAINT_SHARED_SECRET;
    }

    return (string) apply_filters('wc_maintenance_shared_secret', '');
}

function wc_maint_rate_limit_ok(string $ip): bool
{
    $key = 'wc_maint_rl_' . md5($ip);
    $count = (int) get_transient($key);

    if ($count >= 60) {
        return false;
    }

    set_transient($key, $count + 1, MINUTE_IN_SECONDS);
    return true;
}

function wc_maint_health_endpoint(WP_REST_Request $request): WP_REST_Response
{
    return wc_maint_json(array(
        'agent' => wc_maint_agent_info(),
        'health' => wc_maint_health_data(),
    ));
}

function wc_maint_inventory_endpoint(WP_REST_Request $request): WP_REST_Response
{
    wc_maint_load_upgrader_files();

    if ((bool) $request->get_param('refresh')) {
        wp_update_plugins();
        wp_update_themes();
        wp_version_check();
    }

    $installed_plugins = get_plugins();
    $active_plugins = (array) get_option('active_plugins', array());
    $plugin_updates = get_plugin_updates();
    $theme_updates = get_theme_updates();
    $core_update = wc_maint_find_core_update();

    $plugins = array();
    foreach ($installed_plugins as $file => $data) {
        $update = isset($plugin_updates[$file]) ? $plugin_updates[$file] : null;
        $plugins[] = array(
            'file' => $file,
            'name' => isset($data['Name']) ? wp_strip_all_tags($data['Name']) : $file,
            'version' => isset($data['Version']) ? $data['Version'] : null,
            'author' => isset($data['Author']) ? wp_strip_all_tags($data['Author']) : null,
            'active' => in_array($file, $active_plugins, true),
            'update_available' => $update !== null,
            'new_version' => wc_maint_update_target($update),
            'slug' => dirname($file) !== '.' ? dirname($file) : basename($file, '.php'),
        );
    }

    $themes = array();
    foreach (wp_get_themes() as $slug => $theme) {
        $update = isset($theme_updates[$slug]) ? $theme_updates[$slug] : null;
        $themes[] = array(
            'slug' => $slug,
            'name' => $theme->get('Name'),
            'version' => $theme->get('Version'),
            'active' => get_stylesheet() === $slug,
            'update_available' => $update !== null,
            'new_version' => wc_maint_update_target($update),
        );
    }

    return wc_maint_json(array(
        'agent' => wc_maint_agent_info(),
        'generated_at' => gmdate('c'),
        'core' => array(
            'version' => wc_maint_read_wp_version(),
            'update' => $core_update,
        ),
        'plugins' => $plugins,
        'themes' => $themes,
        'health' => wc_maint_health_data(),
    ));
}

function wc_maint_update_plugins_endpoint(WP_REST_Request $request)
{
    wc_maint_load_upgrader_files();

    $dry_run = (bool) $request->get_param('dry_run');
    $backup_confirmed = (bool) $request->get_param('backup_confirmed');
    $continue_on_error = (bool) $request->get_param('continue_on_error');
    $requested = $request->get_param('plugins');

    $requested = is_array($requested)
        ? array_values(array_filter(array_map('sanitize_text_field', $requested)))
        : array();

    $backup_ok = (bool) apply_filters('wc_maintenance_backup_confirmed', $backup_confirmed, $request);
    if (!$dry_run && !$backup_ok) {
        return new WP_Error(
            'wc_maint_backup_missing',
            'Updates sind gesperrt: backup_confirmed muss true sein. Bitte zuerst ein verifiziertes Backup anlegen.',
            array('status' => 409)
        );
    }

    wc_maint_log('plugin_update_start', array(
        'plugins' => $requested,
        'dry_run' => $dry_run,
    ));

    wp_update_plugins();
    $all_plugins = get_plugins();
    $updates = get_plugin_updates();

    if (empty($requested)) {
        $requested = array_keys($updates);
    }

    $planned = array();
    foreach ($requested as $file) {
        if (!isset($all_plugins[$file])) {
            $planned[] = array(
                'file' => $file,
                'error' => 'Plugin ist nicht installiert.',
            );
            continue;
        }

        if (!isset($updates[$file])) {
            $planned[] = array(
                'file' => $file,
                'name' => wp_strip_all_tags($all_plugins[$file]['Name']),
                'version_before' => $all_plugins[$file]['Version'],
                'version_after' => null,
                'skipped' => true,
                'reason' => 'Kein Update verfügbar.',
            );
            continue;
        }

        $planned[] = array(
            'file' => $file,
            'name' => wp_strip_all_tags($all_plugins[$file]['Name']),
            'version_before' => $all_plugins[$file]['Version'],
            'version_after' => wc_maint_update_target($updates[$file]),
            'slug' => dirname($file) !== '.' ? dirname($file) : basename($file, '.php'),
        );
    }

    if ($dry_run) {
        wc_maint_log('plugin_update_dry_run', array('count' => count($planned)));
        return wc_maint_json(array(
            'status' => 'dry_run',
            'dry_run' => true,
            'plugins' => $planned,
            'health' => wc_maint_health_data(),
        ));
    }

    update_option(WC_MAINT_JOB_OPTION, array('started_at' => gmdate('c'), 'type' => 'plugins'), false);
    $GLOBALS['wc_maint_job_active'] = true;

    $results = array();
    foreach ($planned as $item) {
        if (isset($item['error']) || !empty($item['skipped'])) {
            $results[] = $item;
            continue;
        }

        $file = $item['file'];
        $before = (string) $item['version_before'];

        if (!function_exists('WP_Filesystem')) {
            require_once ABSPATH . 'wp-admin/includes/file.php';
        }
        add_filter('filesystem_method', 'wc_maint_filesystem_direct');

        ob_start();
        $upgrader = new Plugin_Upgrader(new Automatic_Upgrader_Skin());
        $result = $upgrader->upgrade($file);
        $output = trim((string) ob_get_clean());

        remove_filter('filesystem_method', 'wc_maint_filesystem_direct');
        wp_clean_plugins_cache(true);
        $after_plugins = get_plugins();
        $after = isset($after_plugins[$file]['Version']) ? $after_plugins[$file]['Version'] : null;

        $error_message = is_wp_error($result) ? $result->get_error_message() : null;
        $success = $error_message === null && $result !== null && $result !== false && $after !== null
            && version_compare($after, $before, '>=');

        $messages = array();
        if (isset($upgrader->skin) && method_exists($upgrader->skin, 'get_upgrade_messages')) {
            $messages = $upgrader->skin->get_upgrade_messages();
        }

        $results[] = array(
            'file' => $file,
            'name' => $item['name'],
            'version_before' => $before,
            'version_after' => $after,
            'success' => $success,
            'error' => $error_message,
            'messages' => array_slice((array) $messages, 0, 10),
            'output' => $output !== '' ? substr($output, 0, 1000) : null,
        );

        if (!$success && !$continue_on_error) {
            break;
        }
    }

    delete_option(WC_MAINT_JOB_OPTION);
    $GLOBALS['wc_maint_job_active'] = false;
    wp_update_plugins();
    wp_update_themes();

    $health = wc_maint_health_check();
    $success_count = 0;
    $failure_count = 0;
    foreach ($results as $result) {
        if (!empty($result['success'])) {
            $success_count++;
        } elseif (isset($result['success']) || isset($result['error'])) {
            $failure_count++;
        }
    }

    if ($failure_count === 0) {
        $status = 'ok';
    } elseif ($success_count > 0) {
        $status = 'partial';
    } else {
        $status = 'failed';
    }

    wc_maint_log('plugin_update_finish', array(
        'status' => $status,
        'success_count' => $success_count,
        'total' => count($results),
    ));

    return wc_maint_json(array(
        'status' => $status,
        'plugins' => $results,
        'health' => $health,
    ));
}

function wc_maint_update_core_endpoint(WP_REST_Request $request)
{
    wc_maint_load_upgrader_files();

    $dry_run = (bool) $request->get_param('dry_run');
    $backup_confirmed = (bool) $request->get_param('backup_confirmed');

    $backup_ok = (bool) apply_filters('wc_maintenance_backup_confirmed', $backup_confirmed, $request);
    if (!$dry_run && !$backup_ok) {
        return new WP_Error(
            'wc_maint_backup_missing',
            'Core-Update ist gesperrt: backup_confirmed muss true sein.',
            array('status' => 409)
        );
    }

    wp_version_check();
    $update = wc_maint_find_core_update();

    if ($update === null) {
        return wc_maint_json(array(
            'status' => 'ok',
            'message' => 'Kein Core-Update verfügbar.',
            'current_version' => wc_maint_read_wp_version(),
            'health' => wc_maint_health_check(),
        ));
    }

    if ($dry_run) {
        return wc_maint_json(array(
            'status' => 'dry_run',
            'dry_run' => true,
            'current_version' => wc_maint_read_wp_version(),
            'target_version' => $update->current,
            'locale' => isset($update->locale) ? $update->locale : null,
            'health' => wc_maint_health_data(),
        ));
    }

    update_option(WC_MAINT_JOB_OPTION, array('started_at' => gmdate('c'), 'type' => 'core'), false);
    $GLOBALS['wc_maint_job_active'] = true;

    if (!function_exists('WP_Filesystem')) {
        require_once ABSPATH . 'wp-admin/includes/file.php';
    }
    add_filter('filesystem_method', 'wc_maint_filesystem_direct');

    $before = wc_maint_read_wp_version();
    ob_start();
    $upgrader = new Core_Upgrader(new Automatic_Upgrader_Skin());
    $result = $upgrader->upgrade($update);
    $output = trim((string) ob_get_clean());
    remove_filter('filesystem_method', 'wc_maint_filesystem_direct');
    delete_option(WC_MAINT_JOB_OPTION);
    $GLOBALS['wc_maint_job_active'] = false;

    $error_message = is_wp_error($result) ? $result->get_error_message() : null;
    $after = wc_maint_read_wp_version();
    $success = $error_message === null && $result !== null && $result !== false;

    $health = wc_maint_health_check();

    wc_maint_log('core_update_finish', array(
        'success' => $success,
        'before' => $before,
        'after' => $after,
        'error' => $error_message,
    ));

    return wc_maint_json(array(
        'status' => $success ? 'ok' : 'failed',
        'version_before' => $before,
        'version_after' => $after,
        'success' => $success,
        'error' => $error_message,
        'output' => $output !== '' ? substr($output, 0, 1000) : null,
        'health' => $health,
    ), $success ? 200 : 500);
}

function wc_maint_log_endpoint(WP_REST_Request $request): WP_REST_Response
{
    $log = get_option(WC_MAINT_LOG_OPTION, array());
    if (!is_array($log)) {
        $log = array();
    }

    return wc_maint_json(array(
        'entries' => array_slice($log, 0, 50),
    ));
}

function wc_maint_agent_info(): array
{
    return array(
        'name' => 'Wartungscockpit Maintenance Agent',
        'version' => WC_MAINT_VERSION,
        'namespace' => WC_MAINT_NAMESPACE,
        'wp_version' => get_bloginfo('version'),
        'php_version' => PHP_VERSION,
        'multisite' => is_multisite(),
        'ssl' => is_ssl(),
    );
}

function wc_maint_health_data(): array
{
    global $wpdb;

    if (!function_exists('get_plugins')) {
        require_once ABSPATH . 'wp-admin/includes/plugin.php';
    }

    $plugin_updates = get_site_transient('update_plugins');
    $theme_updates = get_site_transient('update_themes');
    $core_update = wc_maint_find_core_update();
    $theme = wp_get_theme();

    $plugin_update_count = is_object($plugin_updates) && !empty($plugin_updates->response)
        ? count($plugin_updates->response)
        : 0;
    $theme_update_count = is_object($theme_updates) && !empty($theme_updates->response)
        ? count($theme_updates->response)
        : 0;

    return array(
        'generated_at' => gmdate('c'),
        'site' => array(
            'name' => get_bloginfo('name'),
            'url' => home_url('/'),
            'wp_url' => site_url('/'),
            'is_ssl' => is_ssl(),
            'is_multisite' => is_multisite(),
        ),
        'core' => array(
            'version' => wc_maint_read_wp_version(),
            'update_available' => $core_update !== null,
            'new_version' => $core_update !== null ? $core_update->current : null,
        ),
        'php' => array(
            'version' => PHP_VERSION,
            'memory_limit' => ini_get('memory_limit'),
            'max_execution_time' => ini_get('max_execution_time'),
        ),
        'database' => array(
            'version' => method_exists($wpdb, 'db_version') ? $wpdb->db_version() : null,
        ),
        'disk' => array(
            'free_space' => function_exists('disk_free_space') ? @disk_free_space(ABSPATH) : null,
        ),
        'theme' => array(
            'name' => $theme->get('Name'),
            'version' => $theme->get('Version'),
            'stylesheet' => get_stylesheet(),
        ),
        'plugins' => array(
            'installed' => function_exists('get_plugins') ? count(get_plugins()) : null,
            'active' => count((array) get_option('active_plugins', array())),
            'updates_available' => $plugin_update_count,
        ),
        'themes' => array(
            'updates_available' => $theme_update_count,
        ),
        'debug' => defined('WP_DEBUG') && WP_DEBUG,
    );
}

/**
 * Liest die Zielversion aus einem Update-Datensatz.
 * get_plugin_updates() und get_theme_updates() legen sie unter ->update ab,
 * nicht direkt am Objekt.
 */
function wc_maint_update_target($entry): ?string
{
    if (!is_object($entry)) {
        return null;
    }

    if (isset($entry->update) && is_object($entry->update) && isset($entry->update->new_version)) {
        return (string) $entry->update->new_version;
    }

    if (isset($entry->new_version)) {
        return (string) $entry->new_version;
    }

    return null;
}

function wc_maint_find_core_update()
{
    $core_updates = get_site_transient('update_core');
    if (!is_object($core_updates) || empty($core_updates->updates)) {
        return null;
    }

    foreach ($core_updates->updates as $update) {
        if (isset($update->response) && $update->response === 'upgrade') {
            return $update;
        }
    }

    return null;
}

function wc_maint_health_check(): array
{
    $url = home_url('/');
    $response = wp_remote_get($url, array(
        'timeout' => 15,
        'redirection' => 3,
        'user-agent' => 'Wartungscockpit-Agent/' . WC_MAINT_VERSION,
    ));

    if (is_wp_error($response)) {
        return array(
            'url' => $url,
            'ok' => null,
            'http_status' => null,
            'error' => $response->get_error_message(),
        );
    }

    $status = (int) wp_remote_retrieve_response_code($response);
    $body = substr((string) wp_remote_retrieve_body($response), 0, 500000);
    $fatal_marker = (bool) preg_match('/(Fatal error|Parse error|There has been a critical error)/i', $body);

    return array(
        'url' => $url,
        'ok' => $status >= 200 && $status < 400 && !$fatal_marker,
        'http_status' => $status,
        'fatal_marker' => $fatal_marker,
    );
}

function wc_maint_read_wp_version(): string
{
    $version_file = ABSPATH . WPINC . '/version.php';

    if (is_readable($version_file)) {
        $contents = (string) file_get_contents($version_file);
        if (preg_match('/\$wp_version\s*=\s*[\'"]([^\'"]+)[\'"]/', $contents, $matches)) {
            return $matches[1];
        }
    }

    return (string) get_bloginfo('version');
}

function wc_maint_load_upgrader_files(): void
{
    require_once ABSPATH . 'wp-admin/includes/file.php';
    require_once ABSPATH . 'wp-admin/includes/misc.php';
    require_once ABSPATH . 'wp-admin/includes/plugin.php';
    require_once ABSPATH . 'wp-admin/includes/update.php';
    require_once ABSPATH . 'wp-admin/includes/class-wp-upgrader.php';
    require_once ABSPATH . 'wp-admin/includes/class-plugin-upgrader.php';
    require_once ABSPATH . 'wp-admin/includes/class-core-upgrader.php';
}

function wc_maint_filesystem_direct(): string
{
    return 'direct';
}

function wc_maint_json(array $data, int $status = 200): WP_REST_Response
{
    return new WP_REST_Response($data, $status);
}

function wc_maint_log(string $action, array $data = array()): void
{
    $log = get_option(WC_MAINT_LOG_OPTION, array());
    if (!is_array($log)) {
        $log = array();
    }

    array_unshift($log, array(
        'time' => gmdate('c'),
        'action' => $action,
        'data' => $data,
    ));

    update_option(WC_MAINT_LOG_OPTION, array_slice($log, 0, 100), false);
}

function wc_maint_shutdown_handler(): void
{
    if (empty($GLOBALS['wc_maint_job_active'])) {
        return;
    }

    $error = error_get_last();
    if (!is_array($error)) {
        return;
    }

    $fatal_types = array(E_ERROR, E_PARSE, E_CORE_ERROR, E_COMPILE_ERROR);
    if (!in_array($error['type'], $fatal_types, true)) {
        return;
    }

    wc_maint_log('fatal_error', array(
        'message' => isset($error['message']) ? substr((string) $error['message'], 0, 500) : '',
        'file' => isset($error['file']) ? basename((string) $error['file']) : null,
        'line' => isset($error['line']) ? (int) $error['line'] : null,
    ));

    delete_option(WC_MAINT_JOB_OPTION);
    $GLOBALS['wc_maint_job_active'] = false;
}
