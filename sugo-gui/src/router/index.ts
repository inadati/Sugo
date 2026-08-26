import { createRouter, createWebHistory } from "vue-router";
import ShellLayout from "../layouts/ShellLayout.vue";
import HarnessList from "../views/HarnessList.vue";
import TrashView from "../views/TrashView.vue";
import HarnessView from "../views/HarnessView.vue";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: "/",
      component: ShellLayout,
      children: [
        { path: "", component: HarnessList },
        { path: "uncategorized", component: HarnessList },
        { path: "folder/:id", component: HarnessList },
        { path: "trash", component: TrashView },
      ],
    },
    { path: "/harness/:id", component: HarnessView, props: true },
  ],
});
