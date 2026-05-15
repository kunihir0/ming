const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

// Resolve the root directory (one level up from scripts/)
const rootDir = path.resolve(__dirname, '..');

// Load .env file
const envPath = path.join(rootDir, '.env');
if (fs.existsSync(envPath)) {
    const envFile = fs.readFileSync(envPath, 'utf8');
    envFile.split('\n').forEach(line => {
        const match = line.match(/^\s*([\w.-]+)\s*=\s*(.*)?\s*$/);
        if (match) {
            const key = match[1];
            let value = match[2] || '';
            // Remove quotes if present
            value = value.replace(/^['"](.*)['"]$/, '$1');
            if (!process.env[key]) {
                process.env[key] = value;
            }
        }
    });
    console.log('Loaded .env file.');
}

console.log('Starting Ming services...');

const processes = [];

// Helper to spawn processes cross-platform
function startProcess(name, command, args, cwd) {
    const proc = spawn(command, args, {
        cwd: cwd || rootDir,
        env: process.env,
        stdio: 'inherit',
        shell: true // Needed for cross-platform execution of commands like 'cargo' or 'bun'
    });

    proc.on('error', (err) => {
        console.error(`[${name}] Failed to start:`, err);
    });

    proc.on('exit', (code) => {
        console.log(`[${name}] Exited with code ${code}`);
    });

    processes.push(proc);
    return proc;
}

// Start Bot
startProcess('Bot', 'cargo', ['run', '-p', 'bot']);

// Start API
startProcess('API', 'cargo', ['run', '-p', 'api']);

// Start Web Frontend
const webDir = path.join(rootDir, 'web');
if (fs.existsSync(webDir)) {
    startProcess('Web', 'bun', ['run', 'dev'], webDir);
}

// Handle graceful shutdown
function shutdown() {
    console.log('\nShutting down services...');
    processes.forEach(proc => {
        if (!proc.killed) {
            proc.kill('SIGINT');
        }
    });
    process.exit(0);
}

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);
