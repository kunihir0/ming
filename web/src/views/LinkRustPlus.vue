<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';

const router = useRouter();
const jsonInput = ref('');
const error = ref('');
const success = ref('');

const linkAccount = async () => {
  try {
    error.value = '';
    success.value = '';
    const payload = JSON.parse(jsonInput.value);

    // Make API request to our Rust backend to link the account
    const response = await fetch('/api/auth/rustplus/link', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(payload),
    });

    if (!response.ok) {
      throw new Error(`Server returned ${response.status}: ${await response.text()}`);
    }

    success.value = 'Successfully linked your Rust+ credentials! Redirecting to dashboard...';
    
    setTimeout(() => {
      router.push('/');
    }, 2000);
  } catch (err: any) {
    error.value = `Failed to link account: ${err.message}`;
  }
};
</script>

<template>
  <div class="min-h-screen bg-rp-black text-rp-white font-body flex items-center justify-center p-4">
    <div class="w-full max-w-2xl bg-rp-800 border border-rp-700 p-8 shadow-2xl">
      <h1 class="text-2xl text-rp-accent font-mono mb-4 uppercase tracking-widest">Link Rust+ Account</h1>
      
      <p class="text-sm text-rp-300 mb-6 leading-relaxed">
        Due to browser security restrictions, you must run our secure CLI tool to authenticate with Steam and generate your Rust+ credentials.
      </p>

      <div class="bg-rp-900 border border-rp-700 p-4 mb-6">
        <h2 class="text-sm font-mono text-rp-400 mb-2 uppercase">Instructions</h2>
        <ol class="list-decimal list-inside text-sm text-rp-200 space-y-2">
          <li>Open your terminal/command prompt.</li>
          <li>Navigate to the project root directory.</li>
          <li>Run the following command:
            <code class="block bg-rp-black text-rp-accent p-2 mt-1 mb-1 border border-rp-700">cargo run -p cli</code>
          </li>
          <li>A browser window will open. Log in to Steam.</li>
          <li>The terminal will output a block of JSON credentials. Copy the entire JSON output.</li>
          <li>Paste the JSON into the box below and click Link.</li>
        </ol>
      </div>

      <div class="space-y-4">
        <div>
          <label class="block text-xs font-mono text-rp-400 uppercase mb-2">Paste Credentials JSON</label>
          <textarea 
            v-model="jsonInput" 
            class="w-full h-48 bg-rp-black border border-rp-700 text-rp-200 p-3 font-mono text-xs focus:outline-none focus:border-rp-accent focus:ring-1 focus:ring-rp-accent transition-colors"
            placeholder="{&#10;  &quot;fcm_credentials&quot;: { ... },&#10;  &quot;expo_push_token&quot;: &quot;...&quot;,&#10;  &quot;rustplus_auth_token&quot;: &quot;...&quot;&#10;}"
          ></textarea>
        </div>

        <div v-if="error" class="bg-red-900/50 border border-red-500 text-red-200 p-3 text-sm">
          {{ error }}
        </div>

        <div v-if="success" class="bg-green-900/50 border border-green-500 text-green-200 p-3 text-sm">
          {{ success }}
        </div>

        <button 
          @click="linkAccount"
          class="w-full bg-rp-700 hover:bg-rp-accent hover:text-rp-black text-rp-white transition-colors duration-200 border border-rp-600 p-3 font-mono uppercase tracking-wider text-sm"
        >
          Link Account
        </button>
      </div>
    </div>
  </div>
</template>