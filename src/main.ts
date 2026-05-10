import { createApp } from "vue";
import App from "@/App.vue";
import { installErrorBridge } from "@utils/error-bridge";
import "@fontsource-variable/material-symbols-outlined/full.css";
import "@/styles/tailwind.css";

installErrorBridge();

createApp(App).mount("#app");
