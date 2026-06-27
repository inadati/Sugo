import { createRouter, createWebHistory } from "vue-router";
import HarnessList from "../views/HarnessList.vue";
import HarnessView from "../views/HarnessView.vue";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", component: HarnessList },
    { path: "/harness/:id", component: HarnessView, props: true },
  ],
});
